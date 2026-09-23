mod import_cmd;

use anyhow::{anyhow, bail};
use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use vfiles_app::{
    AdminService, AuthService, BootstrapService, HealthService, HistoryService, MaintenanceService,
    SearchService, SessionService, ShareService, UploadService, WorkspaceService,
};
use vfiles_config::{ConfigLoader, MIN_MAINTENANCE_INTERVAL_SECONDS, MaintenanceConfig};
use vfiles_domain::*;
use vfiles_ftp::{
    BackendDeps, FtpApplication, FtpSettings, RoleFilter, VfilesAuthenticator,
    VfilesUserDetailProvider,
};
use vfiles_http::{AppState, FrontendAssets, build_router, middleware::LoginAttemptLimiter};
use vfiles_infra_fs::FsStorageBootstrap;
use vfiles_infra_sqlite::{
    SqliteAuditLogRepo, SqliteFavoriteRepo, SqliteHealthProbe, SqliteMigrations, SqlitePoolFactory,
    repo::*,
};
use vfiles_s3::VfilesS3;

#[derive(Debug, Parser)]
#[command(name = "vfiles")]
#[command(about = "VFiles server")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start the HTTP server
    Serve(ServeArgs),
    /// Initialize storage and database
    Init,
    /// Register the server as an OS service
    Register(RegisterArgs),
    /// Manage users
    User {
        #[command(subcommand)]
        command: UserCommands,
    },
    /// Import a local directory into a user's namespace
    Import(ImportArgs),
    /// Run health checks
    Check,
    /// Maintenance tasks
    Maintenance {
        #[command(subcommand)]
        command: MaintenanceCommands,
    },
}

#[derive(Debug, Subcommand)]
enum MaintenanceCommands {
    /// Delete unreferenced blobs (with a grace period) to reclaim disk space
    GcBlobs(GcBlobsArgs),
    /// Keep only the newest N snapshots per namespace, then reclaim their blobs
    PruneSnapshots(PruneSnapshotsArgs),
}

#[derive(Debug, Args, Clone)]
struct GcBlobsArgs {
    /// Only purge blobs created more than this many seconds ago
    #[arg(long, default_value_t = 3600)]
    grace_seconds: u64,
}

#[derive(Debug, Args, Clone)]
struct PruneSnapshotsArgs {
    /// Number of newest snapshots to keep per namespace
    #[arg(long, default_value_t = 50)]
    keep: u32,
    /// Only prune snapshots older than this many days (0 disables the age window)
    #[arg(long, default_value_t = 0)]
    older_than_days: u32,
    /// Only purge blobs created more than this many seconds ago
    #[arg(long, default_value_t = 3600)]
    grace_seconds: u64,
}

#[derive(Debug, Args, Clone)]
struct ImportArgs {
    /// 源目录（服务器上的本地路径）
    source: PathBuf,
    /// 目标目录（命名空间内路径，默认根目录）
    #[arg(long, default_value = "")]
    target: String,
    /// 归属用户（默认取默认命名空间的所有者）
    #[arg(long)]
    owner: Option<String>,
    /// 快照策略：batch / per-file / off
    #[arg(long, default_value = "batch")]
    snapshot_mode: String,
    /// batch 模式下每多少个文件提交一次快照
    #[arg(long, default_value_t = 200)]
    flush_files: usize,
    /// 即使内容未变化也生成新版本
    #[arg(long)]
    force: bool,
    /// 只统计不写入
    #[arg(long)]
    dry_run: bool,
    /// 单文件大小上限（字节），默认使用服务端 limits 配置
    #[arg(long)]
    max_file_size_bytes: Option<u64>,
    /// 跳过以 . 开头的隐藏文件
    #[arg(long)]
    exclude_hidden: bool,
}

#[derive(Debug, Args, Clone)]
struct ServeArgs {
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    port: Option<u16>,
}

#[derive(Debug, Subcommand)]
enum UserCommands {
    /// Create a user
    Create(UserCreateArgs),
    /// List users
    List(UserListArgs),
    /// Get a single user by ID
    Get(UserGetArgs),
    /// Update an existing user
    Update(UserUpdateArgs),
    /// Delete a user by ID
    Delete(UserDeleteArgs),
    /// Reset a user's password (recovery when locked out)
    ResetPassword(UserResetPasswordArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum RegisterTargetArg {
    Systemd,
}

#[derive(Debug, Args)]
struct RegisterArgs {
    #[arg(short = 't', long = "type", value_enum)]
    target: RegisterTargetArg,
    #[arg(long, default_value = "vfiles")]
    service_name: String,
    #[arg(short = 'w', long)]
    working_directory: Option<PathBuf>,
    #[arg(short = 'd', long)]
    data_directory: Option<PathBuf>,
    #[arg(long)]
    start: bool,
}

#[derive(Debug)]
struct SystemdUnitConfig<'a> {
    service_name: &'a str,
    executable_path: &'a Path,
    working_directory: &'a Path,
    data_directory: Option<&'a Path>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum UserRoleArg {
    Admin,
    Manager,
    User,
}

impl From<UserRoleArg> for Role {
    fn from(value: UserRoleArg) -> Self {
        match value {
            UserRoleArg::Admin => Role::Admin,
            UserRoleArg::Manager => Role::Manager,
            UserRoleArg::User => Role::User,
        }
    }
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("user_create_password")
        .required(true)
        .args(["password", "password_hash"])
))]
struct UserCreateArgs {
    #[arg(short, long)]
    username: String,
    #[arg(short, long)]
    email: String,
    #[arg(long, value_enum, default_value_t = UserRoleArg::User)]
    role: UserRoleArg,
    #[arg(long, conflicts_with = "password_hash")]
    password: Option<String>,
    #[arg(short, long, conflicts_with = "password")]
    password_hash: Option<String>,
}

#[derive(Debug, Args)]
struct UserResetPasswordArgs {
    /// 按用户名定位账号（与 --user-id 二选一；忘记账号时先用 user list 查）
    #[arg(short, long)]
    username: Option<String>,
    /// 按用户 ID 定位账号
    #[arg(long)]
    user_id: Option<String>,
    /// 新密码（至少 8 位）
    #[arg(long, conflicts_with_all = ["password_hash", "generate"])]
    password: Option<String>,
    /// 直接提供已哈希的密码（供自动化使用）
    #[arg(short, long, conflicts_with = "generate")]
    password_hash: Option<String>,
    /// 生成随机强密码并打印出来
    #[arg(long)]
    generate: bool,
    /// 保留该用户已登录的会话（默认会全部下线）
    #[arg(long)]
    keep_sessions: bool,
}

#[derive(Debug, Args)]
struct UserListArgs {
    #[arg(long, default_value_t = 1)]
    page: i64,
    #[arg(long, default_value_t = 50)]
    page_size: i64,
}

#[derive(Debug, Args)]
struct UserGetArgs {
    user_id: String,
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("user_update_status")
        .args(["enable", "disable"])
))]
struct UserUpdateArgs {
    user_id: String,
    #[arg(long)]
    email: Option<String>,
    /// 修改用户名（修复历史数据里未通过校验的用户名，例如带 `-` 的账号）
    #[arg(short, long)]
    username: Option<String>,
    #[arg(long, value_enum)]
    role: Option<UserRoleArg>,
    #[arg(long)]
    enable: bool,
    #[arg(long)]
    disable: bool,
}

#[derive(Debug, Args)]
struct UserDeleteArgs {
    user_id: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 日志默认 info 级别（RUST_LOG 仍可覆盖）。此前未设置 RUST_LOG 时
    // EnvFilter 会禁用所有事件，出错（缩略图解码失败、维护任务异常等）也完全静默。
    let log_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(log_filter).init();

    let cli = Cli::parse();

    tracing::info!("VFiles server starting with command: {:?}", cli.command);

    match cli.command {
        Commands::Serve(args) => {
            run_serve(args).await?;
        }
        Commands::Init => {
            run_init().await?;
        }
        Commands::Register(args) => {
            run_register(args)?;
        }
        Commands::User { command } => {
            run_user_command(command).await?;
        }
        Commands::Import(args) => {
            run_import_command(args).await?;
        }
        Commands::Check => {
            run_check().await?;
        }
        Commands::Maintenance { command } => {
            run_maintenance_command(command).await?;
        }
    }

    Ok(())
}

async fn run_init() -> anyhow::Result<()> {
    tracing::info!("Starting initialization...");
    let config = ConfigLoader::load()?;
    let paths = vfiles_config::AppPaths::from_config(&config.storage);

    tracing::info!("Ensuring data directory exists: {}", paths.storage_root);
    // Ensure data directory exists
    std::fs::create_dir_all(&paths.storage_root).map_err(|e| {
        anyhow::anyhow!(
            "Failed to create data directory {}: {}",
            paths.storage_root,
            e
        )
    })?;

    // Pre-create the database file to ensure it exists
    if !paths.database.exists() {
        tracing::info!("Creating database file: {}", paths.database);
        std::fs::File::create(&paths.database).map_err(|e| {
            anyhow::anyhow!("Failed to create database file {}: {}", paths.database, e)
        })?;
    } else {
        tracing::debug!("Database file already exists: {}", paths.database);
    }

    // Bootstrap storage
    tracing::info!("Bootstrapping storage structure...");
    FsStorageBootstrap::bootstrap(&paths.storage_root).await?;

    // Connect to database
    tracing::info!("Connecting to database for migrations...");
    let pool = SqlitePoolFactory::connect(&paths.database).await?;

    // Run migrations
    tracing::info!("Running database migrations...");
    SqliteMigrations::run(&pool).await?;

    tracing::info!("Initialization complete");
    Ok(())
}

fn run_register(args: RegisterArgs) -> anyhow::Result<()> {
    match args.target {
        RegisterTargetArg::Systemd => register_systemd_service(args),
    }
}

fn register_systemd_service(args: RegisterArgs) -> anyhow::Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = args;
        bail!("systemd registration is only supported on Linux");
    }

    #[cfg(target_os = "linux")]
    {
        validate_service_name(&args.service_name)?;

        let current_dir = std::env::current_dir()
            .map_err(|e| anyhow!("failed to determine current directory: {}", e))?;
        let executable_path = std::env::current_exe()
            .map_err(|e| anyhow!("failed to determine current executable path: {}", e))?;
        let working_directory = absolutize_path(
            args.working_directory
                .as_deref()
                .unwrap_or(current_dir.as_path()),
            &current_dir,
        );

        if !working_directory.is_dir() {
            bail!(
                "working directory does not exist or is not a directory: {}",
                working_directory.display()
            );
        }

        let data_directory = args
            .data_directory
            .as_deref()
            .map(|path| absolutize_path(path, &current_dir));
        if let Some(data_directory) = &data_directory {
            std::fs::create_dir_all(data_directory).map_err(|e| {
                anyhow!(
                    "failed to create data directory {}: {}",
                    data_directory.display(),
                    e
                )
            })?;
        }

        let unit_path =
            PathBuf::from("/etc/systemd/system").join(format!("{}.service", args.service_name));
        let unit_contents = render_systemd_unit(&SystemdUnitConfig {
            service_name: &args.service_name,
            executable_path: &executable_path,
            working_directory: &working_directory,
            data_directory: data_directory.as_deref(),
        });

        std::fs::write(&unit_path, unit_contents).map_err(|e| {
            anyhow!(
                "failed to write systemd unit file {}: {}. try running the command with sufficient permissions",
                unit_path.display(),
                e
            )
        })?;

        run_systemctl(["daemon-reload"])?;
        run_systemctl(["enable", &args.service_name])?;
        if args.start {
            run_systemctl(["start", &args.service_name])?;
        }

        println!("Registered systemd service: {}", args.service_name);
        println!("Unit file: {}", unit_path.display());
        println!("Working directory: {}", working_directory.display());
        if let Some(data_directory) = data_directory {
            println!("Data directory: {}", data_directory.display());
        }
        if args.start {
            println!("Service started");
        } else {
            println!(
                "Run `sudo systemctl start {}` to start it now.",
                args.service_name
            );
        }

        Ok(())
    }
}

fn validate_service_name(service_name: &str) -> anyhow::Result<()> {
    let trimmed = service_name.trim();
    if trimmed.is_empty() {
        bail!("service name cannot be empty");
    }

    if trimmed.contains('/') || trimmed.contains('\\') {
        bail!(
            "service name cannot contain path separators: {}",
            service_name
        );
    }

    if trimmed.chars().any(char::is_whitespace) {
        bail!("service name cannot contain whitespace: {}", service_name);
    }

    Ok(())
}

fn absolutize_path(path: &Path, base_dir: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

fn render_systemd_unit(config: &SystemdUnitConfig<'_>) -> String {
    let mut unit = format!(
        "[Unit]\nDescription=VFiles service ({})\nAfter=network.target\n\n[Service]\nType=simple\nExecStart={} serve\nWorkingDirectory={}\nRestart=on-failure\nRestartSec=5\n",
        config.service_name,
        systemd_quote(&config.executable_path.display().to_string()),
        config.working_directory.display(),
    );

    if let Some(data_directory) = config.data_directory {
        unit.push_str(&format!(
            "Environment={}\n",
            systemd_quote(&format!("VFILES_STORAGE_ROOT={}", data_directory.display()))
        ));
    }

    unit.push_str("\n[Install]\nWantedBy=multi-user.target\n");
    unit
}

fn systemd_quote(raw: &str) -> String {
    let mut quoted = String::with_capacity(raw.len() + 2);
    quoted.push('"');
    for ch in raw.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

fn run_systemctl<const N: usize>(args: [&str; N]) -> anyhow::Result<()> {
    let status = Command::new("systemctl")
        .args(args)
        .status()
        .map_err(|e| anyhow!("failed to run `systemctl {}`: {}", args.join(" "), e))?;

    if !status.success() {
        bail!(
            "`systemctl {}` exited with status {}",
            args.join(" "),
            status
        );
    }

    Ok(())
}

struct UserCommandContext {
    user_repo: SqliteUserRepo,
    admin_repo: SqliteAdminRepo,
    admin_service: AdminService<SqliteAdminRepo>,
    bootstrap_service:
        BootstrapService<SqliteUserRepo, SqliteNamespaceRepo, SqliteSystemSettingsRepo>,
}

async fn build_user_command_context() -> anyhow::Result<UserCommandContext> {
    let config = ConfigLoader::load()?;
    let paths = vfiles_config::AppPaths::from_config(&config.storage);
    let pool = SqlitePoolFactory::connect(&paths.database).await?;

    let user_repo = SqliteUserRepo::new(pool.clone());
    let admin_repo = SqliteAdminRepo::new(pool.clone());
    let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
    let settings_repo = SqliteSystemSettingsRepo::new(pool.clone());
    let session_repo = SqliteSessionRepo::new(pool);
    let auth_service = AuthService::new(
        user_repo.clone(),
        session_repo,
        config.auth.session_ttl_seconds,
    );

    Ok(UserCommandContext {
        user_repo: user_repo.clone(),
        admin_repo: admin_repo.clone(),
        admin_service: AdminService::new(admin_repo, auth_service),
        bootstrap_service: BootstrapService::new(user_repo, namespace_repo, settings_repo),
    })
}

async fn run_user_command(command: UserCommands) -> anyhow::Result<()> {
    match command {
        UserCommands::Create(args) => run_user_create(args).await,
        UserCommands::List(args) => run_user_list(args).await,
        UserCommands::Get(args) => run_user_get(args).await,
        UserCommands::Update(args) => run_user_update(args).await,
        UserCommands::Delete(args) => run_user_delete(args).await,
        UserCommands::ResetPassword(args) => run_user_reset_password(args).await,
    }
}

async fn run_user_create(args: UserCreateArgs) -> anyhow::Result<()> {
    let context = build_user_command_context().await?;
    let role: Role = args.role.into();
    let admin_count = context.admin_service.count_admins().await?;

    if admin_count == 0 {
        if !role.is_admin() {
            bail!("system is not bootstrapped; first user must be created with --role admin");
        }

        let password_hash =
            resolve_password_hash_input(args.password.as_deref(), args.password_hash.as_deref())?;
        let user_id = context
            .bootstrap_service
            .bootstrap_admin(&args.username, &args.email, &password_hash)
            .await?;
        println!("Bootstrapped admin user: {}", user_id);
        return Ok(());
    }

    let user_id = match (args.password.as_deref(), args.password_hash.as_deref()) {
        (Some(password), None) => {
            context
                .admin_service
                .create_user(vfiles_app::CreateUserRequest {
                    username: args.username,
                    email: args.email,
                    password: password.to_string(),
                    role,
                })
                .await?
        }
        (None, Some(password_hash)) => {
            create_user_with_password_hash(
                &context.user_repo,
                &context.admin_repo,
                &args.username,
                &args.email,
                password_hash,
                role,
            )
            .await?
        }
        (Some(_), Some(_)) => unreachable!("clap enforces mutual exclusion"),
        (None, None) => unreachable!("clap enforces one password input"),
    };

    println!("Created user: {}", user_id);
    Ok(())
}

async fn run_user_list(args: UserListArgs) -> anyhow::Result<()> {
    if args.page < 1 || args.page_size < 1 {
        bail!("--page and --page-size must be positive integers");
    }

    let context = build_user_command_context().await?;
    let user_list = context
        .admin_service
        .list_users(args.page, args.page_size)
        .await?;

    println!(
        "total_count={} page={} page_size={}",
        user_list.total_count, user_list.page, user_list.page_size
    );
    println!("id\tusername\temail\trole\tdisabled\tcreated_at");
    for user in user_list.users {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            user.id,
            user.username,
            user.email.as_deref().unwrap_or("-"),
            user.role,
            user.disabled,
            user.created_at
        );
    }

    Ok(())
}

async fn run_user_get(args: UserGetArgs) -> anyhow::Result<()> {
    let context = build_user_command_context().await?;
    let user_id = parse_user_id(&args.user_id)?;
    let user = context.admin_service.get_user_details(&user_id).await?;

    println!("id: {}", user.id);
    println!("username: {}", user.username);
    println!("email: {}", user.email.as_deref().unwrap_or("-"));
    println!("role: {}", user.role);
    println!("disabled: {}", user.disabled);
    println!("created_at: {}", user.created_at);
    println!(
        "last_login: {}",
        user.last_login
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );

    Ok(())
}

/// 生成随机密码（不含易混淆字符，长度 20）。
///
/// 随机源用 `uuid::Uuid::new_v4()`（getrandom），并做拒绝采样避免取模偏置。
fn generate_password() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#%^&*-_";
    const LENGTH: usize = 20;
    let limit = 256 - (256 % ALPHABET.len());

    let mut password = String::with_capacity(LENGTH);
    'outer: for _ in 0..4 {
        for byte in uuid::Uuid::new_v4().as_bytes() {
            let value = *byte as usize;
            if value >= limit {
                continue;
            }
            password.push(ALPHABET[value % ALPHABET.len()] as char);
            if password.len() == LENGTH {
                break 'outer;
            }
        }
    }
    password
}

/// `vfiles user reset-password`：忘记密码时的离线恢复入口。
///
/// 会先打印账号信息（用户名 / 邮箱 / 角色 / 状态）便于确认，再重置密码；
/// 默认让该用户所有会话失效，避免旧会话继续可用。
async fn run_user_reset_password(args: UserResetPasswordArgs) -> anyhow::Result<()> {
    let context = build_user_command_context().await?;

    let user = match (args.username.as_deref(), args.user_id.as_deref()) {
        (Some(username), None) => context
            .admin_service
            .find_user_by_username(username)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user not found: {username}"))?,
        (None, Some(raw_id)) => {
            let user_id = parse_user_id(raw_id)?;
            context.admin_service.find_user(&user_id).await?
        }
        (Some(_), Some(_)) => bail!("--username and --user-id cannot be used together"),
        (None, None) => bail!("either --username or --user-id must be provided"),
    };

    // 先打印账号信息，便于确认「找到的是哪个管理员」
    println!(
        "user: id={} username={} email={} role={} disabled={}",
        user.id,
        user.username,
        user.email.as_deref().unwrap_or("-"),
        user.role,
        user.disabled
    );

    let generated = args.generate;
    // 明文密码（--generate 时随机生成）或已哈希密码，二选一
    let plaintext = if generated {
        Some(generate_password())
    } else {
        args.password.clone()
    };
    let password_hash = args.password_hash.clone();

    if let Some(hash) = password_hash.as_deref() {
        context
            .admin_service
            .set_password_hash(&user.id, hash)
            .await?;
    } else {
        let password = plaintext
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("provide --password, --password-hash or --generate"))?;
        context
            .admin_service
            .reset_password(&user.id, password)
            .await?;
    }

    if !args.keep_sessions {
        context.admin_service.revoke_user_sessions(&user.id).await?;
    }

    if let Some(password) = plaintext.as_deref().filter(|_| generated) {
        println!("new_password={password}");
    }
    println!(
        "Password reset for {} ({}); sessions {}",
        user.username,
        user.id,
        if args.keep_sessions {
            "kept"
        } else {
            "revoked"
        }
    );
    Ok(())
}

async fn run_user_update(args: UserUpdateArgs) -> anyhow::Result<()> {
    let disabled = if args.enable {
        Some(false)
    } else if args.disable {
        Some(true)
    } else {
        None
    };
    let role = args.role.map(Into::into);

    if args.email.is_none() && role.is_none() && disabled.is_none() && args.username.is_none() {
        bail!("no update requested; provide --username, --email, --role, --enable or --disable");
    }

    let context = build_user_command_context().await?;
    let user_id = parse_user_id(&args.user_id)?;
    context
        .admin_service
        .update_user(
            &user_id,
            vfiles_app::UpdateUserRequest {
                role,
                disabled,
                email: args.email,
                username: args.username,
            },
        )
        .await?;

    println!("Updated user: {}", user_id);
    Ok(())
}

async fn run_user_delete(args: UserDeleteArgs) -> anyhow::Result<()> {
    let context = build_user_command_context().await?;
    let user_id = parse_user_id(&args.user_id)?;
    context.admin_service.delete_user(&user_id).await?;

    println!("Deleted user: {}", user_id);
    Ok(())
}

async fn create_user_with_password_hash(
    user_repo: &SqliteUserRepo,
    admin_repo: &SqliteAdminRepo,
    username: &str,
    email: &str,
    password_hash: &str,
    role: Role,
) -> DomainResult<UserId> {
    let username = Username::new(username)?;
    let email = EmailAddress::new(email)?;

    if user_repo.find_by_username(&username).await.is_ok() {
        return Err(DomainError::Conflict {
            message: "Username already exists".to_string(),
        });
    }

    if user_repo.find_by_email(&email).await.is_ok() {
        return Err(DomainError::Conflict {
            message: "Email already exists".to_string(),
        });
    }

    admin_repo
        .create_user(&username, &email, password_hash, role)
        .await
}

fn parse_user_id(raw: &str) -> anyhow::Result<UserId> {
    UserId::from_string(raw).map_err(|_| anyhow!("invalid user ID: {}", raw))
}

fn resolve_password_hash_input(
    password: Option<&str>,
    password_hash: Option<&str>,
) -> anyhow::Result<String> {
    match (password, password_hash) {
        (Some(password), None) => Ok(AuthService::hash_password_for_storage(password)?),
        (None, Some(password_hash)) => Ok(password_hash.to_string()),
        (Some(_), Some(_)) => bail!("--password and --password-hash cannot be used together"),
        (None, None) => bail!("either --password or --password-hash must be provided"),
    }
}

/// `vfiles import`：把本地目录导入到某个用户的命名空间。
///
/// 复用 `ImportBatch`，因此大批量导入只按阈值提交快照；整个流程不开放网络端口。
async fn run_import_command(args: ImportArgs) -> anyhow::Result<()> {
    let config = ConfigLoader::load()?;
    ConfigLoader::validate(&config)?;
    let paths = vfiles_config::AppPaths::from_config(&config.storage);

    let pool = SqlitePoolFactory::connect(&paths.database).await?;
    // 与 serve 一致：老库可能缺少新表，导入前补齐迁移
    SqliteMigrations::run(&pool).await?;

    let user_repo: Arc<dyn UserRepo + Send + Sync> = Arc::new(SqliteUserRepo::new(pool.clone()));
    let namespace_repo: Arc<dyn NamespaceRepo + Send + Sync> =
        Arc::new(SqliteNamespaceRepo::new(pool.clone()));
    let entry_repo: Arc<dyn EntryRepo + Send + Sync> = Arc::new(SqliteEntryRepo::new(pool.clone()));
    let snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync> =
        Arc::new(SqliteSnapshotRepo::new(pool.clone()));
    let blob_store: Arc<dyn BlobStore + Send + Sync> =
        Arc::new(FsBlobStore::new(pool.clone(), paths.blobs.clone()));

    let upload_store = FsUploadStore::new(paths.uploads.clone());
    let workspace = Arc::new(vfiles_app::DefaultWorkspaceService::new(
        SqliteEntryRepo::new(pool.clone()),
        SqliteSnapshotRepo::new(pool.clone()),
        FsBlobStore::new(pool.clone(), paths.blobs.clone()),
        upload_store,
    ));

    let owner = match args.owner.clone() {
        Some(owner) => owner,
        None => {
            import_cmd::resolve_default_owner(user_repo.as_ref(), namespace_repo.as_ref())
                .await?
                .1
        }
    };

    let snapshot_mode = match args.snapshot_mode.as_str() {
        "batch" => vfiles_app::SnapshotMode::Batch,
        "per-file" => vfiles_app::SnapshotMode::PerFile,
        "off" => vfiles_app::SnapshotMode::Off,
        other => bail!("未知的快照策略: {other}（可选 batch / per-file / off）"),
    };

    let options = import_cmd::ImportOptions {
        source: args.source.clone(),
        target: args.target.clone(),
        owner: owner.clone(),
        snapshot_mode,
        flush_files: args.flush_files.max(1),
        skip_unchanged: !args.force,
        dry_run: args.dry_run,
        max_file_size_bytes: Some(
            args.max_file_size_bytes
                .unwrap_or(config.limits.max_file_size_bytes),
        ),
        include_hidden: !args.exclude_hidden,
    };

    tracing::info!(source = %args.source.display(), owner = %owner, "开始导入本地目录");

    let report = import_cmd::run_import(
        import_cmd::ImportDeps {
            workspace,
            entry_repo,
            snapshot_repo,
            blob_store,
            user_repo,
            namespace_repo,
        },
        options,
    )
    .await?;

    // 有失败项时以非零退出码结束，便于脚本判断
    if report.has_errors() {
        bail!("导入完成，但有 {} 项失败", report.errors.len());
    }

    if args.dry_run {
        tracing::info!(
            files = report.files,
            directories = report.directories,
            bytes = report.bytes,
            "预览完成（未写入数据）"
        );
    }

    Ok(())
}

async fn run_check() -> anyhow::Result<()> {
    tracing::info!("Running health checks...");
    let config = ConfigLoader::load()?;
    let paths = vfiles_config::AppPaths::from_config(&config.storage);

    // Check config
    tracing::debug!("Validating configuration...");
    ConfigLoader::validate(&config)?;

    // Check database
    tracing::debug!("Checking database connectivity...");
    let pool = SqlitePoolFactory::connect(&paths.database).await?;
    SqliteHealthProbe::check_readiness(&pool).await?;

    // Check storage
    tracing::debug!("Checking storage accessibility...");
    vfiles_infra_fs::FsHealthProbe::check_readiness(&paths.storage_root).await?;

    tracing::info!("All health checks passed");
    Ok(())
}

fn is_frontend_dist(path: &Path) -> bool {
    path.is_dir() && path.join("index.html").is_file()
}

async fn run_maintenance_command(command: MaintenanceCommands) -> anyhow::Result<()> {
    match command {
        MaintenanceCommands::GcBlobs(args) => run_gc_blobs(args).await,
        MaintenanceCommands::PruneSnapshots(args) => run_prune_snapshots(args).await,
    }
}

/// 清理无引用且已过保护期的 blob，回收磁盘空间。
async fn run_gc_blobs(args: GcBlobsArgs) -> anyhow::Result<()> {
    tracing::info!(
        "Purging orphaned blobs (grace period: {}s)...",
        args.grace_seconds
    );
    let config = ConfigLoader::load()?;
    let paths = vfiles_config::AppPaths::from_config(&config.storage);
    let pool = SqlitePoolFactory::connect(&paths.database).await?;

    let blob_store = FsBlobStore::new(pool.clone(), paths.blobs.clone());
    let entry_repo = SqliteEntryRepo::new(pool.clone());
    let snapshot_repo = SqliteSnapshotRepo::new(pool.clone());
    let service = MaintenanceService::new(blob_store, entry_repo, snapshot_repo);
    let report = service.purge_orphan_blobs(args.grace_seconds).await?;

    println!(
        "Purged {} orphaned blob(s), freed {} bytes",
        report.removed, report.freed_bytes
    );
    tracing::info!(
        removed = report.removed,
        freed_bytes = report.freed_bytes,
        "Blob garbage collection finished"
    );

    pool.close().await;
    Ok(())
}

/// 裁剪历史快照（每个命名空间保留最新 N 个），随后回收孤儿 blob。
async fn run_prune_snapshots(args: PruneSnapshotsArgs) -> anyhow::Result<()> {
    tracing::info!(
        "Pruning snapshots (keep {} per namespace, older than {} days)...",
        args.keep,
        args.older_than_days
    );
    let config = ConfigLoader::load()?;
    let paths = vfiles_config::AppPaths::from_config(&config.storage);
    let pool = SqlitePoolFactory::connect(&paths.database).await?;

    let blob_store = FsBlobStore::new(pool.clone(), paths.blobs.clone());
    let entry_repo = SqliteEntryRepo::new(pool.clone());
    let snapshot_repo = SqliteSnapshotRepo::new(pool.clone());
    let service = MaintenanceService::new(blob_store, entry_repo, snapshot_repo);

    // vfiles-bin 没有直接依赖 time crate，这里用手写时长避免新增依赖
    let older_than = (args.older_than_days > 0)
        .then(|| std::time::Duration::from_secs(u64::from(args.older_than_days) * 86_400));
    let report = service
        .prune_snapshots_with_age(args.keep, older_than)
        .await?;
    let purge = service.purge_orphan_blobs(args.grace_seconds).await?;

    println!(
        "Pruned {} snapshot(s), released {} blob(s); purged {} orphaned blob(s), freed {} bytes",
        report.pruned_snapshots, report.released_blobs, purge.removed, purge.freed_bytes
    );
    tracing::info!(
        pruned_snapshots = report.pruned_snapshots,
        released_blobs = report.released_blobs,
        purged_blobs = purge.removed,
        freed_bytes = purge.freed_bytes,
        "Snapshot pruning finished"
    );

    pool.close().await;
    Ok(())
}

fn resolve_frontend_dist() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("VFILES_FRONTEND_DIST") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let candidate = PathBuf::from(trimmed);
            if is_frontend_dist(&candidate) {
                return Some(candidate);
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let mut current = Some(cwd.as_path());
        while let Some(dir) = current {
            let candidate = dir.join("client").join("dist");
            if is_frontend_dist(&candidate) {
                return Some(candidate);
            }
            if dir.join("Cargo.toml").exists() {
                break;
            }
            current = dir.parent();
        }
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join("client").join("dist");
        if is_frontend_dist(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn resolve_frontend_assets() -> Option<FrontendAssets> {
    if let Some(frontend_dist) = resolve_frontend_dist() {
        return FrontendAssets::filesystem(frontend_dist);
    }

    #[cfg(feature = "embed")]
    {
        return FrontendAssets::embedded();
    }

    #[allow(unreachable_code)]
    None
}

async fn run_serve(args: ServeArgs) -> anyhow::Result<()> {
    println!("Entering run_serve function...");
    tracing::info!("Loading configuration...");
    let mut config = ConfigLoader::load()?;
    if let Some(host) = args.host {
        config.http.host = host;
    }
    if let Some(port) = args.port {
        config.http.port = port;
    }
    let paths = vfiles_config::AppPaths::from_config(&config.storage);
    let frontend_assets = resolve_frontend_assets();

    tracing::info!(
        "Configuration loaded - host: {}, port: {}",
        config.http.host,
        config.http.port
    );
    tracing::info!(
        "Storage paths - root: {}, database: {}",
        paths.storage_root,
        paths.database
    );
    if let Some(frontend_assets) = &frontend_assets {
        tracing::info!(
            "Serving static frontend from {}",
            frontend_assets.describe()
        );
    } else {
        tracing::warn!("No frontend dist found; backend will serve API routes only");
    }

    // Connect to database
    tracing::info!("Connecting to database...");
    let pool = SqlitePoolFactory::connect(&paths.database).await?;
    tracing::info!("Database connection established");

    // 启动时补齐迁移：老库（例如只在早期版本 init 过的库）不会自动获得新表/索引，
    // 之前仅在 `vfiles init` 里跑迁移，导致升级二进制后 serve 时报「表不存在」。
    tracing::info!("Running database migrations...");
    SqliteMigrations::run(&pool).await?;
    tracing::info!("Database migrations up to date");

    // Create repos
    tracing::debug!("Creating repository instances...");
    let user_repo = SqliteUserRepo::new(pool.clone());
    let namespace_repo = SqliteNamespaceRepo::new(pool.clone());
    let _settings_repo = SqliteSystemSettingsRepo::new(pool.clone());
    let session_repo = SqliteSessionRepo::new(pool.clone());

    // Create repositories
    let entry_repo = SqliteEntryRepo::new(pool.clone());
    let snapshot_repo = SqliteSnapshotRepo::new(pool.clone());
    let share_repo = SqliteShareRepo::new(pool.clone());
    let admin_repo = SqliteAdminRepo::new(pool.clone());
    let blob_store = FsBlobStore::new(pool.clone(), paths.blobs.clone());
    let upload_store = FsUploadStore::new(paths.uploads.clone());
    let search_repo = SqliteSearchRepo::new(pool.clone(), blob_store.clone());
    let favorite_repo = SqliteFavoriteRepo::new(pool.clone());

    // Create services
    tracing::debug!("Creating service instances...");
    let health_service = HealthService;
    let auth_service = AuthService::new(
        user_repo.clone(),
        session_repo.clone(),
        config.auth.session_ttl_seconds,
    );
    let admin_service = AdminService::new(admin_repo, auth_service.clone());
    let session_service = SessionService::new(Some(auth_service.clone()), config.features.clone());
    let search_service = SearchService::new(search_repo);
    let share_service = ShareService::new(share_repo, entry_repo.clone());
    let history_service = HistoryService::new(
        entry_repo.clone(),
        snapshot_repo.clone(),
        blob_store.clone(),
        user_repo.clone(),
    );
    let workspace_service = WorkspaceService::new(
        entry_repo.clone(),
        snapshot_repo.clone(),
        blob_store.clone(),
        upload_store.clone(),
    );
    let upload_service = UploadService::new(
        entry_repo.clone(),
        snapshot_repo.clone(),
        blob_store.clone(),
        upload_store.clone(),
    );

    // Get or create default namespace - for now, assume it exists
    // Get default namespace
    tracing::debug!("Finding default namespace...");
    let default_namespace_id = namespace_repo.find_default().await?;
    tracing::info!("Default namespace found: {}", default_namespace_id);
    let default_actor_user_id: String =
        sqlx::query_scalar("SELECT owner_user_id FROM namespaces WHERE id = ? LIMIT 1")
            .bind(default_namespace_id.to_string())
            .fetch_one(&pool)
            .await?;
    let default_actor_user_id = UserId::from_uuid(uuid::Uuid::parse_str(&default_actor_user_id)?);

    // 维护服务在 AppState 之前构建（后者的字段会 move 走 repo/blob store）
    let maintenance_service = MaintenanceService::new(
        blob_store.clone(),
        entry_repo.clone(),
        snapshot_repo.clone(),
    );
    let maintenance_schedule = config
        .maintenance
        .enabled
        .then(|| MaintenanceSchedule::from_config(&config.maintenance));

    // FTP 与 HTTP 共用同一份限流器与统计实例（计数、封禁策略一致）
    let ingest_stats = Arc::new(vfiles_app::IngestStats::new());
    let login_attempt_limiter = Arc::new(LoginAttemptLimiter::new());

    // FTP 需要与 HTTP 相同的仓储视图：这里先建立共享 Arc，AppState 与 FTP 各持一份
    let entry_repo_arc: Arc<dyn EntryRepo + Send + Sync> = Arc::new(entry_repo.clone());
    let snapshot_repo_arc: Arc<dyn SnapshotRepo + Send + Sync> = Arc::new(snapshot_repo.clone());
    let blob_store_arc: Arc<dyn BlobStore + Send + Sync> = Arc::new(blob_store.clone());
    let user_repo_arc: Arc<dyn UserRepo + Send + Sync> = Arc::new(user_repo.clone());

    // FTP 的导入批次需要独立的 WorkspaceService 组合（与 HTTP 的实例共享底层仓储）
    let ftp_workspace = Arc::new(WorkspaceService::new(
        entry_repo.clone(),
        snapshot_repo.clone(),
        blob_store.clone(),
        upload_store.clone(),
    ));

    let ftp_runtime = build_ftp_runtime(
        &config,
        FtpRuntimeDeps {
            auth_service: auth_service.clone(),
            workspace: Arc::clone(&ftp_workspace),
            entry_repo: Arc::clone(&entry_repo_arc),
            snapshot_repo: Arc::clone(&snapshot_repo_arc),
            blob_store: Arc::clone(&blob_store_arc),
            user_repo: Arc::clone(&user_repo_arc),
            // SqliteNamespaceRepo 不派生 Clone，这里用同一个连接池重建
            namespace_repo: Arc::new(SqliteNamespaceRepo::new(pool.clone())),
            login_attempt_limiter: Arc::clone(&login_attempt_limiter),
            stats: Arc::clone(&ingest_stats),
        },
    )?;

    // WebDAV（r110'a ✓ 用户令「默认开启」+ auth 防御（config 层 r109b ✓））
    let webdav_runtime = build_webdav_runtime(
        config.webdav.enabled,
        config.webdav.bind_address(),
        auth_service.clone(),
        vfiles_app::NamespaceService::new(Arc::new(SqliteNamespaceRepo::new(pool.clone()))),
        Arc::clone(&entry_repo_arc),
        Arc::clone(&ftp_workspace),
        upload_service.clone(),
        {
            // r5 审计闭包（run_serve 有 pool ✓ 构造后传参（独立函数无 pool ✗ #45 作用域））
            let audit_service = std::sync::Arc::new(vfiles_app::AuditService::new(
                SqliteAuditLogRepo::new(pool.clone()),
            ));
            Some(std::sync::Arc::new(move |entry| {
                let svc = audit_service.clone();
                tokio::spawn(async move { svc.record(entry).await });
            }))
        },
        config.webdav.embedded,
        config.webdav.mount_path.clone(),
    );

    // 主停机信号（前移到 upload move 与 AppState 构造之前 ✗ S3 调用点需要两者皆活 +
    // rx 已定义 = 依赖序矛盾解 ✗ 顶层作用域位置变 = 后续分发点照常可见）
    let (service_shutdown_tx, service_shutdown_rx) = tokio::sync::watch::channel(false);

    // S3 兼容 API（默认启用 ✗ VFILES_S3_ENABLED=false 可关闭）
    let embedded_s3 = if config.s3.enabled {
        let service = build_s3_service(
            &config.s3,
            Arc::clone(&entry_repo_arc),
            std::sync::Arc::new(vfiles_infra_sqlite::SqliteNamespaceRepo::new(pool.clone())),
            Arc::clone(&ftp_workspace),
            upload_service.clone(),
            default_namespace_id,
            default_actor_user_id,
        )
        .await;
        if config.s3.embedded {
            tracing::info!("S3 兼容 API 已挂载到 HTTP 主端口（SigV4 请求分流）");
            Some(service)
        } else {
            spawn_s3_server(service, &config.s3, service_shutdown_rx.clone());
            None
        }
    } else {
        tracing::info!("S3 兼容 API 已按配置关闭（VFILES_S3_ENABLED=false）");
        None
    };

    // rsync daemon（默认启用、只读 ✗ VFILES_RSYNC_ENABLED=false 可关闭）
    if config.rsync.enabled {
        build_and_spawn_rsync(
            &config.rsync,
            Arc::clone(&entry_repo_arc),
            Arc::clone(&ftp_workspace),
            upload_service.clone(),
            default_namespace_id,
            default_actor_user_id,
            service_shutdown_rx.clone(),
        );
    } else {
        tracing::info!("rsync daemon 已按配置关闭（VFILES_RSYNC_ENABLED=false）");
    }

    // Create app state
    let app_state = AppState {
        health_service,
        session_service,
        auth_service: Some(auth_service),
        admin_service: Some(admin_service),
        search_service,
        share_service,
        history_service,
        workspace_service,
        upload_service,
        db_pool: pool.clone(),
        namespace_repo: Arc::new(namespace_repo),
        entry_repo: Arc::clone(&entry_repo_arc),
        favorite_repo: std::sync::Arc::new(favorite_repo),
        audit_service: vfiles_app::AuditService::new(SqliteAuditLogRepo::new(pool.clone())),
        user_repo: Arc::new(SqliteUserRepo::new(pool.clone())),
        access_token_service: vfiles_app::AccessTokenService::new(Arc::new(
            SqliteAccessTokenRepo::new(pool.clone()),
        )),
        ownership_service: vfiles_app::OwnershipService::new(
            Arc::clone(&entry_repo_arc),
            Arc::new(SqliteNamespaceRepo::new(pool.clone())),
            Arc::new(SqliteUserRepo::new(pool.clone())),
            Arc::clone(&snapshot_repo_arc),
        ),
        snapshot_repo: Arc::clone(&snapshot_repo_arc),
        blob_store: Arc::clone(&blob_store_arc),
        upload_store: std::sync::Arc::new(upload_store),
        login_attempt_limiter: Arc::clone(&login_attempt_limiter),
        ingest_stats: Arc::clone(&ingest_stats),
        share_download_limiter: Arc::new(vfiles_http::FixedWindowLimiter::new()),
        default_namespace_id,
        default_actor_user_id,
        frontend_assets,
        config: config.clone(),
    };

    // Build router
    tracing::debug!("Building HTTP router...");
    let frontend_for_dispatch = app_state.frontend_assets.clone();
    let mut app = if let Some(s3_service) = embedded_s3 {
        vfiles_http::build_router_without_frontend(app_state)
            .fallback(shared_http_s3_fallback)
            .layer(axum::Extension(SharedS3Dispatch {
                service: s3_service,
                frontend: frontend_for_dispatch,
            }))
    } else {
        build_router(app_state)
    };
    // 共端口装配（r-new ✓ S1 实证：axum 0.8 nest_service + into_service 原生可用 ✗
    // 异 state 挂入、nest 自动剥前缀 = handlers 零改 ✓ 显式路由优先于 fallback ✓）
    if let Some((_, webdav_app)) = webdav_runtime.clone() {
        if !webdav_app.mount_prefix.is_empty() {
            let mount = webdav_app.mount_prefix.clone();
            tracing::info!(
                mount = %mount,
                "WebDAV 已挂载（共端口模式 ✓ 主端口同时提供 HTTP API + WebDAV + 前端）"
            );
            app = app.nest_service(
                &mount,
                vfiles_webdav::router_for_e2e(webdav_app).into_service(),
            );
        }
    }

    // Start server
    let addr = format!("{}:{}", config.http.host, config.http.port);
    println!("About to bind to address: {}", addr);
    tracing::info!("Binding to address: {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on http://{}", addr);

    // 周期性维护：与 HTTP 服务并行运行，停机时先取消再关闭连接池
    let (maintenance_shutdown_tx, maintenance_shutdown_rx) = tokio::sync::watch::channel(false);
    let maintenance_task = match maintenance_schedule {
        Some(schedule) => {
            tracing::info!(
                interval_seconds = schedule.interval.as_secs(),
                initial_delay_seconds = schedule.initial_delay.as_secs(),
                blob_grace_seconds = schedule.blob_grace_seconds,
                snapshot_keep = schedule.snapshot_keep,
                "Periodic maintenance enabled"
            );
            Some(tokio::spawn(run_maintenance_loop(
                maintenance_service,
                schedule,
                maintenance_shutdown_rx,
            )))
        }
        None => {
            tracing::debug!("Periodic maintenance disabled (VFILES_MAINTENANCE_ENABLED=false)");
            None
        }
    };

    // HTTP 与 FTP 共享同一个停机信号：任意一个收到 SIGTERM/SIGINT 都开始优雅停机

    // WebDAV spawn（r110'a ✓ 降级式 = FTP 同款韧性（端口占用不拖垮站点 ✓））
    let webdav_handle = match webdav_runtime {
        Some((settings, application)) => {
            if !application.mount_prefix.is_empty() {
                // 共端口模式（r-new ✓ 挂载日志在装配处 ✗ 不再独立监听 = 无 bind 面）
                tracing::info!(
                    mount = %application.mount_prefix,
                    "WebDAV 共端口模式：跳过独立监听（就绪以主端口「已挂载」行为准）"
                );
                None
            } else {
                let bind = settings.bind.clone();
                match vfiles_webdav::spawn_webdav_server(
                    settings,
                    application,
                    service_shutdown_rx.clone(),
                ) {
                    Ok(()) => {
                        // 调度式文案（r205 ✓ bind 成功以「监听就绪」为权威 ✗ "已启用"曾在 bind 失败时误导）
                        tracing::info!(
                            "WebDAV 监听任务已调度: {bind}（成功确认行 = 「WebDAV 监听就绪」）"
                        );
                        Some(())
                    }
                    Err(err) => {
                        tracing::error!(%bind, error = %err, "WebDAV 启动失败，继续提供其余服务");
                        None
                    }
                }
            }
        }
        None => None,
    };
    let _ = webdav_handle;

    let ftp_handle = match ftp_runtime {
        Some((settings, application)) => {
            // FTP 默认开启：启动失败（例如端口被占用）不能让整个站点起不来，
            // 这里降级为错误日志并继续提供 HTTP 服务。
            let bind = settings.bind;
            match vfiles_ftp::spawn_ftp_server(settings, application, service_shutdown_rx.clone())
                .await
            {
                Ok(handle) => {
                    tracing::info!("FTP 批量导入已启用: {}", handle.local_addr());
                    Some(handle)
                }
                Err(err) => {
                    tracing::error!(
                        %bind,
                        error = %err,
                        "FTP 批量导入启动失败，已跳过；HTTP 服务继续运行（可设置 VFILES_FTP_ENABLED=false 消除该错误）"
                    );
                    None
                }
            }
        }
        None => {
            tracing::debug!("FTP 未启用（VFILES_FTP_ENABLED=false 或认证已关闭）");
            None
        }
    };

    // 把 ctrl-c/SIGTERM 转发为共享停机信号
    {
        let shutdown_tx = service_shutdown_tx.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            let _ = shutdown_tx.send(true);
        });
    }

    tracing::info!("VFiles server started successfully!");
    let mut http_shutdown = service_shutdown_rx.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // 收到信号或 FTP 侧触发停机时都结束 HTTP 服务
            let _ = http_shutdown.changed().await;
        })
        .await?;

    // 收到 SIGTERM/SIGINT 后先停止接收新请求，再停止 FTP 与维护任务，最后关闭连接池。
    let _ = service_shutdown_tx.send(true);
    if let Some(handle) = ftp_handle {
        handle.wait().await;
    }
    tracing::info!("Shutdown signal received; stopping maintenance and closing database pool");
    let _ = maintenance_shutdown_tx.send(true);
    if let Some(task) = maintenance_task
        && let Err(err) = task.await
    {
        tracing::warn!(error = %err, "maintenance task did not stop cleanly");
    }
    pool.close().await;
    tracing::info!("VFiles server stopped");

    Ok(())
}

/// 等待 Ctrl+C（SIGINT）或 SIGTERM，用于优雅停机。
/// 周期性维护的运行参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MaintenanceSchedule {
    interval: Duration,
    initial_delay: Duration,
    blob_grace_seconds: u64,
    snapshot_keep: u32,
    /// 快照最长保留天数（0 表示不按时间裁剪，只按数量）。
    snapshot_max_age_days: u32,
}

impl MaintenanceSchedule {
    fn from_config(config: &MaintenanceConfig) -> Self {
        Self {
            // 配置已做过下限收敛，这里再兜一层，避免绕过配置构造出忙循环
            interval: Duration::from_secs(
                config
                    .interval_seconds
                    .max(MIN_MAINTENANCE_INTERVAL_SECONDS),
            ),
            initial_delay: Duration::from_secs(config.effective_initial_delay_seconds()),
            blob_grace_seconds: config.blob_grace_seconds,
            snapshot_keep: config.snapshot_keep,
            snapshot_max_age_days: config.snapshot_max_age_days,
        }
    }
}

type Maintenance = MaintenanceService<FsBlobStore, SqliteEntryRepo, SqliteSnapshotRepo>;

/// 周期执行维护，直到收到停机信号。
///
/// 首次执行等待 `initial_delay`（默认 5 分钟），避免与启动/重启时的
/// 其他 I/O 叠加；此后按 `interval` 执行，单次失败只记录告警并在下个周期重试。
async fn run_maintenance_loop(
    service: Maintenance,
    schedule: MaintenanceSchedule,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    if !schedule.initial_delay.is_zero() {
        tracing::debug!(
            initial_delay_seconds = schedule.initial_delay.as_secs(),
            "Waiting before the first maintenance run"
        );
        tokio::select! {
            _ = tokio::time::sleep(schedule.initial_delay) => {}
            _ = shutdown.changed() => return,
        }
    }

    // 首轮在 initial_delay 之后立即执行
    run_maintenance_tick(&service, &schedule).await;

    let mut ticker = tokio::time::interval(schedule.interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // interval 的第一拍立即到期（上面已经跑过首轮），这里丢弃它
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => run_maintenance_tick(&service, &schedule).await,
            _ = shutdown.changed() => {
                tracing::info!("Periodic maintenance stopped");
                return;
            }
        }
    }
}

/// 执行一次维护并记录结果；失败只告警，不终止服务。
async fn run_maintenance_tick(service: &Maintenance, schedule: &MaintenanceSchedule) {
    match service
        .run_once(
            schedule.blob_grace_seconds,
            schedule.snapshot_keep,
            schedule.snapshot_max_age_days,
        )
        .await
    {
        Ok(report) => {
            tracing::info!(
                pruned_snapshots = report.pruned_snapshots,
                released_blobs = report.released_blobs,
                purged_blobs = report.purged_blobs,
                freed_bytes = report.freed_bytes,
                "Periodic maintenance finished"
            );
        }
        Err(err) => {
            tracing::warn!(error = %err, "Periodic maintenance failed; retrying next interval");
        }
    }
}

/// 构建 FTP 运行时所需的依赖。
struct FtpRuntimeDeps {
    auth_service: AuthService,
    workspace: Arc<vfiles_app::DefaultWorkspaceService>,
    entry_repo: Arc<dyn EntryRepo + Send + Sync>,
    snapshot_repo: Arc<dyn SnapshotRepo + Send + Sync>,
    blob_store: Arc<dyn BlobStore + Send + Sync>,
    user_repo: Arc<dyn UserRepo + Send + Sync>,
    namespace_repo: Arc<dyn NamespaceRepo + Send + Sync>,
    login_attempt_limiter: Arc<LoginAttemptLimiter>,
    stats: Arc<vfiles_app::IngestStats>,
}

/// WebDAV 写门面 → workspace service 转发（r110'a ✓ bin 侧具体型 ✓ crate 免耦合）。
struct WebdavWrite {
    workspace: std::sync::Arc<vfiles_app::DefaultWorkspaceService>,
    upload: vfiles_app::UploadService<
        vfiles_infra_sqlite::SqliteEntryRepo,
        vfiles_infra_sqlite::SqliteSnapshotRepo,
        vfiles_infra_sqlite::FsBlobStore,
        vfiles_infra_sqlite::FsUploadStore,
    >,
}

#[async_trait::async_trait]
impl vfiles_webdav::WebdavWriteOps for WebdavWrite {
    async fn mkcol(
        &self,
        ns: &vfiles_domain::NamespaceId,
        path: &vfiles_domain::NormalizedPath,
        uid: &vfiles_domain::UserId,
    ) -> vfiles_domain::DomainResult<()> {
        self.workspace
            .create_directory(ns, path, Some("WebDAV MKCOL"), uid)
            .await
            .map(|_| ())
    }
    async fn copy_entry(
        &self,
        ns: &vfiles_domain::NamespaceId,
        source: &vfiles_domain::NormalizedPath,
        destination: &vfiles_domain::NormalizedPath,
        user_id: &vfiles_domain::UserId,
        overwrite: bool,
    ) -> vfiles_domain::DomainResult<()> {
        // r5 薄转发 ✗ 树逻辑在 services.copy_entries（blob 复用 + 递归 + r10 覆盖链 ✓）
        self.workspace
            .copy_entries(
                ns,
                source,
                destination,
                Some("WebDAV COPY"),
                user_id,
                overwrite,
            )
            .await
    }

    async fn move_entry(
        &self,
        ns: &vfiles_domain::NamespaceId,
        from: &vfiles_domain::NormalizedPath,
        to: &vfiles_domain::NormalizedPath,
        uid: &vfiles_domain::UserId,
    ) -> vfiles_domain::DomainResult<()> {
        self.workspace
            .move_entries(
                ns,
                std::slice::from_ref(from),
                to,
                Some("WebDAV MOVE"),
                uid,
                false, // Path（WebDAV Destination = 完整目标路径 ✗ r12）
            )
            .await
            .map(|_| ())
    }
    async fn get_stream(
        &self,
        ns: &vfiles_domain::NamespaceId,
        path: &vfiles_domain::NormalizedPath,
    ) -> vfiles_domain::DomainResult<
        Option<(Box<dyn vfiles_domain::ReadSeek + Send + Unpin>, String, u64)>,
    > {
        // 流式直通（r201 ✓ reader 不落内存 ✓ open_file 同链）
        let file = match self.workspace.open_file(ns, path, None).await {
            Ok(f) => f,
            Err(vfiles_domain::DomainError::NotFound { .. }) => return Ok(None),
            Err(err) => return Err(err),
        };
        let mime = file
            .mime_type
            .unwrap_or_else(|| "application/octet-stream".to_string());
        Ok(Some((file.reader, mime, file.size_bytes)))
    }
    async fn put_file(
        &self,
        ns: &vfiles_domain::NamespaceId,
        path: &vfiles_domain::NormalizedPath,
        data: Vec<u8>,
        uid: &vfiles_domain::UserId,
    ) -> vfiles_domain::DomainResult<()> {
        // init_upload 语义：target_path = 父目录 + filename = 文件名（r110'c 修正：此前
        // 误传完整路径导致文件被建成目录条目）。
        let full = path.as_str();
        let (parent_str, filename) = match full.rsplit_once('/') {
            Some((dir, name)) => (dir.to_string(), name.to_string()),
            None => (String::new(), full.to_string()),
        };
        let filename = if filename.is_empty() {
            "upload".to_string()
        } else {
            filename
        };
        let parent = vfiles_domain::NormalizedPath::new(&parent_str).map_err(|err| {
            vfiles_domain::DomainError::Validation {
                message: format!("路径非法：{err}"),
            }
        })?;
        let session = self
            .upload
            .init_upload(ns, &parent, &filename, data.len() as u64, None, None, uid)
            .await?;
        self.upload
            .complete_upload_from_stream(
                &session.upload_id,
                None,
                Some("WebDAV PUT"),
                Box::new(std::io::Cursor::new(data)),
            )
            .await?;
        Ok(())
    }
    async fn delete_entry(
        &self,
        ns: &vfiles_domain::NamespaceId,
        path: &vfiles_domain::NormalizedPath,
        uid: &vfiles_domain::UserId,
    ) -> vfiles_domain::DomainResult<()> {
        self.workspace
            .delete_entries(ns, std::slice::from_ref(path), Some("WebDAV DELETE"), uid)
            .await
            .map(|_| ())
    }
}

/// S3 SigV4 密钥对认证（round 2 ✗ env 静态单对：access 匹配 → 回 secret，供 s3s 内建
/// SigV4 验签重算比对 ✗ 空键已在 runtime 层随机生成 + warn（零配置试用 ✓））。
struct EnvAuth {
    keys: std::collections::HashMap<String, S3Cred>,
}

#[async_trait::async_trait]
impl s3s::auth::S3Auth for EnvAuth {
    async fn get_secret_key(&self, access_key: &str) -> s3s::S3Result<s3s::auth::SecretKey> {
        self.keys
            .get(access_key)
            .map(|c| c.secret.clone())
            .ok_or_else(|| s3s::s3_error!(InvalidAccessKeyId, "unknown access key"))
    }
}

/// 单个 S3 凭证的策略（密钥 + 可选命名空间绑定 + 只读）。
struct S3Cred {
    secret: s3s::auth::SecretKey,
    /// 绑定的命名空间 slug（None = 默认命名空间）。
    namespace: Option<String>,
    readonly: bool,
}

/// 装配 S3 凭证表（单对 `ACCESS_KEY/SECRET_KEY` + 多用 `CREDENTIALS=a:s,b:s2:ro,c:s3:ns:d:s4:ns:ro`；
/// 全空 = 随机生成一把并 warn 打印 access ✗ secret 不落日志）。
fn load_s3_credentials(cfg: &vfiles_config::S3Config) -> std::collections::HashMap<String, S3Cred> {
    let mut keys = std::collections::HashMap::new();
    fn cred(secret: String, namespace: Option<String>, readonly: bool) -> S3Cred {
        S3Cred {
            secret: s3s::auth::SecretKey::from(secret),
            namespace,
            readonly,
        }
    }
    let add = |keys: &mut std::collections::HashMap<String, S3Cred>,
               access: String,
               secret: String,
               namespace: Option<String>,
               readonly: bool| {
        keys.insert(access, cred(secret, namespace, readonly));
    };
    if !cfg.access_key.is_empty() && !cfg.secret_key.is_empty() {
        add(
            &mut keys,
            cfg.access_key.clone(),
            cfg.secret_key.clone(),
            None,
            false,
        );
    }
    // 条目形：`access:secret[:namespace][:ro|rw]`（第三段非模式词即命名空间 slug）
    for entry in cfg.credentials.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let parts: Vec<&str> = entry.split(':').map(|p| p.trim()).collect();
        if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
            tracing::warn!(
                "S3 CREDENTIALS 条目格式非法（应为 access:secret[:namespace][:ro]），已跳过"
            );
            continue;
        }
        let mode = |v: &str| {
            matches!(
                v.to_ascii_lowercase().as_str(),
                "ro" | "readonly" | "read-only" | "rw" | "readwrite" | "read-write"
            )
        };
        let readonly_mode = |v: &str| {
            matches!(
                v.to_ascii_lowercase().as_str(),
                "ro" | "readonly" | "read-only"
            )
        };
        let (namespace, readonly) = match parts.len() {
            2 => (None, false),
            3 if mode(parts[2]) => (None, readonly_mode(parts[2])),
            3 => (Some(parts[2].to_string()), false),
            _ => {
                if !mode(parts[3]) {
                    tracing::warn!(entry = %entry, "S3 CREDENTIALS 第四段非 ro/rw，按读写处理");
                }
                (Some(parts[2].to_string()), readonly_mode(parts[3]))
            }
        };
        add(
            &mut keys,
            parts[0].to_string(),
            parts[1].to_string(),
            namespace,
            readonly,
        );
    }
    if keys.is_empty() {
        let a = format!("VF{}", uuid::Uuid::new_v4().simple());
        tracing::warn!(
            access_key = %a,
            "S3 未配置凭证：已随机生成（打印 access ✗ secret 见启动调试 env；生产请设 VFILES_S3_ACCESS_KEY/SECRET_KEY 或 VFILES_S3_CREDENTIALS）"
        );
        add(
            &mut keys,
            a,
            uuid::Uuid::new_v4().simple().to_string(),
            None,
            false,
        );
    }
    keys
}

type RsyncUploadService = vfiles_app::UploadService<
    vfiles_infra_sqlite::SqliteEntryRepo,
    vfiles_infra_sqlite::SqliteSnapshotRepo,
    vfiles_infra_sqlite::FsBlobStore,
    vfiles_infra_sqlite::FsUploadStore,
>;

/// 装配 rsync daemon 认证（`auth users` 逗号清单 + `secrets file` 的 `user:password` 行）。
fn load_rsync_auth(cfg: &vfiles_config::RsyncConfig) -> vfiles_rsync::AuthConfig {
    let users: Vec<String> = cfg
        .auth_users
        .split(',')
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .collect();
    let mut secrets = std::collections::HashMap::new();
    if !cfg.secrets_file.is_empty() {
        match std::fs::read_to_string(&cfg.secrets_file) {
            Ok(text) => {
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((user, pass)) = line.split_once(':') {
                        secrets.insert(user.trim().to_string(), pass.trim().to_string());
                    }
                }
            }
            Err(err) => tracing::error!(
                path = %cfg.secrets_file,
                error = %err,
                "rsync secrets 文件读取失败（认证将全部拒绝）"
            ),
        }
    }
    if !users.is_empty() && secrets.is_empty() {
        tracing::warn!("rsync 配置了 auth_users 但无可用 secrets（连接将全部认证失败）");
    }
    vfiles_rsync::AuthConfig {
        users,
        secrets,
        writable: cfg.writable,
    }
}

/// rsync 数据后端（domain 链装配 ✗ 下载/上传/删除三面同源）。
struct RepoBackend {
    repo: std::sync::Arc<dyn vfiles_domain::EntryRepo + Send + Sync>,
    workspace: std::sync::Arc<vfiles_app::DefaultWorkspaceService>,
    upload: RsyncUploadService,
    namespace: vfiles_domain::NamespaceId,
    owner: vfiles_domain::UserId,
    /// 连接级 stat 缓存（一次 `files_with_meta` 覆盖全树 ✗ 避免逐文件点查）。
    stat_cache: tokio::sync::OnceCell<std::collections::HashMap<String, (u64, i64)>>,
}

#[async_trait::async_trait]
impl vfiles_rsync::RsyncBackend for RepoBackend {
    async fn list(
        &self,
        req: vfiles_rsync::ListRequest,
    ) -> Result<Vec<vfiles_rsync::FlatEntry>, String> {
        vfiles_rsync::collect_flat(&*self.repo, &self.namespace, &req).await
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let np = vfiles_domain::NormalizedPath::new(path).map_err(|e| e.to_string())?;
        let content = self
            .workspace
            .read_file_bytes(&self.namespace, &np, None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(content.bytes)
    }

    async fn open(
        &self,
        path: &str,
    ) -> Result<Option<(Box<dyn vfiles_domain::ReadSeek + Send + Unpin>, u64)>, String> {
        let np = vfiles_domain::NormalizedPath::new(path).map_err(|e| e.to_string())?;
        let content = self
            .workspace
            .open_file(&self.namespace, &np, None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Some((content.reader, content.size_bytes)))
    }

    async fn stat(&self, path: &str) -> Result<Option<(u64, i64)>, String> {
        let map = self
            .stat_cache
            .get_or_try_init(|| async {
                let metas = self
                    .repo
                    .files_with_meta(&self.namespace)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>(
                    metas
                        .into_iter()
                        .map(|m| {
                            (
                                m.entry.path_norm.as_str().to_string(),
                                (
                                    m.size_bytes.unwrap_or(0),
                                    m.source_mtime.unwrap_or(i64::MIN),
                                ),
                            )
                        })
                        .collect(),
                )
            })
            .await?;
        Ok(map.get(path).copied())
    }

    async fn write(&self, path: String, data: Vec<u8>, mtime: i64) -> Result<(), String> {
        let size = data.len() as u64;
        self.write_stream(path, size, mtime, Box::new(std::io::Cursor::new(data)))
            .await
    }

    async fn write_stream(
        &self,
        path: String,
        size: u64,
        mtime: i64,
        reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    ) -> Result<(), String> {
        let (parent_str, filename) = match path.rsplit_once('/') {
            Some((d, n)) => (d.to_string(), n.to_string()),
            None => (String::new(), path.clone()),
        };
        let filename = if filename.is_empty() {
            "upload".to_string()
        } else {
            filename
        };
        let parent = vfiles_domain::NormalizedPath::new(&parent_str).map_err(|e| e.to_string())?;
        let session = self
            .upload
            .init_upload(
                &self.namespace,
                &parent,
                &filename,
                size,
                None,
                None,
                &self.owner,
            )
            .await
            .map_err(|e| e.to_string())?;
        let result = self
            .upload
            .complete_upload_from_stream(&session.upload_id, None, Some("rsync push"), reader)
            .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if let Err(cancel_error) = self.upload.cancel_upload(&session.upload_id).await {
                    tracing::warn!(error = %cancel_error, upload_id = %session.upload_id, "rsync：清理失败上传会话失败");
                }
                return Err(error.to_string());
            }
        };
        // 记录源端 mtime（rsync `-a` size+mtime 快跳；失败仅告警 = 退化为每次传输）
        if let Err(e) = self
            .repo
            .set_version_source_mtime(&result.version.id, mtime)
            .await
        {
            tracing::warn!(error = %e, path = %path, "rsync：记录源 mtime 失败");
        }
        Ok(())
    }

    async fn delete(&self, paths: Vec<String>) -> Result<(), String> {
        // 先点查存在性（delete_entries 任一缺失即整体 NotFound ✗ 与 S3 同坑）
        let mut existing = Vec::new();
        for p in &paths {
            let Ok(np) = vfiles_domain::NormalizedPath::new(p) else {
                continue;
            };
            match self.repo.find_by_path(&self.namespace, &np).await {
                Ok(Some(_)) => existing.push(np),
                Ok(None) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        if existing.is_empty() {
            return Ok(());
        }
        match self
            .workspace
            .delete_entries(
                &self.namespace,
                &existing,
                Some("rsync --delete"),
                &self.owner,
            )
            .await
        {
            Ok(_) | Err(vfiles_domain::DomainError::NotFound { .. }) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn mkdir(&self, path: &str) -> Result<(), String> {
        let np = vfiles_domain::NormalizedPath::new(path).map_err(|e| e.to_string())?;
        if np.as_str().is_empty() {
            return Ok(());
        }
        match self.repo.find_by_path(&self.namespace, &np).await {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
        self.workspace
            .create_directory(&self.namespace, &np, Some("rsync mkdir"), &self.owner)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

fn build_and_spawn_rsync(
    cfg: &vfiles_config::RsyncConfig,
    entry_repo: std::sync::Arc<dyn vfiles_domain::EntryRepo + Send + Sync>,
    workspace: std::sync::Arc<vfiles_app::DefaultWorkspaceService>,
    upload: RsyncUploadService,
    namespace: vfiles_domain::NamespaceId,
    owner: vfiles_domain::UserId,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let addr = cfg.bind_address();
    let module = cfg.module.clone();
    let auth = load_rsync_auth(cfg);
    tracing::info!(
        addr = %addr,
        module = %module,
        writable = auth.writable,
        auth_users = auth.users.len(),
        "rsync daemon 已拉起（push 门控 + secrets 认证 ✗ 空 auth_users = 匿名）"
    );
    tokio::spawn(async move {
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, peer)) => {
                                let module = module.clone();
                                let repo = std::sync::Arc::clone(&entry_repo);
                                let ws = std::sync::Arc::clone(&workspace);
                                let upload = upload.clone();
                                let auth = auth.clone();
                                let ns = namespace;
                                tokio::spawn(async move {
                                    let backend = RepoBackend {
                                        repo,
                                        workspace: ws,
                                        upload,
                                        namespace: ns,
                                        owner,
                                        stat_cache: tokio::sync::OnceCell::new(),
                                    };
                                    let res = vfiles_rsync::handle_conn(
                                        stream,
                                        &module,
                                        auth,
                                        &backend,
                                    )
                                    .await;
                                    if let Err(err) = res {
                                        tracing::debug!(%peer, error = %err, "rsync 连接结束");
                                    }
                                });
                            }
                            Err(err) => {
                                tracing::warn!(%addr, error = %err, "rsync accept 失败");
                            }
                        }
                    }
                    _ = shutdown.changed() => {
                        tracing::info!(%addr, "rsync daemon 收到停机信号，停止接受新连接");
                        break;
                    }
                }
            },
            Err(err) => {
                // r205 式降级韧性（与 S3/WebDAV 观测语义对齐 ✗ 不拖垮主站）
                tracing::error!(%addr, error = %err, "rsync 绑定失败（端口占用或地址非法），继续提供其余服务");
            }
        }
    });
}

/// 装配 S3 服务（独立端口与共端口共用认证、凭证路由和协议实现）。
// 装配参数天然多（配置/条目仓储/命名空间仓储/工作区/上传/ns/owner/停机 ✗ 打包收益不抵样板）
#[allow(clippy::too_many_arguments)]
async fn build_s3_service(
    cfg: &vfiles_config::S3Config,
    entry_repo: std::sync::Arc<dyn vfiles_domain::EntryRepo + Send + Sync>,
    namespace_repo: std::sync::Arc<dyn vfiles_domain::NamespaceRepo + Send + Sync>,
    workspace: std::sync::Arc<vfiles_app::DefaultWorkspaceService>,
    upload: vfiles_app::UploadService<
        vfiles_infra_sqlite::SqliteEntryRepo,
        vfiles_infra_sqlite::SqliteSnapshotRepo,
        vfiles_infra_sqlite::FsBlobStore,
        vfiles_infra_sqlite::FsUploadStore,
    >,
    namespace: vfiles_domain::NamespaceId,
    owner: vfiles_domain::UserId,
) -> s3s::service::S3Service {
    // 凭证表（多客户端/轮换 + 只读键 + 命名空间绑定 ✗ 空 = 运行时随机 + warn）
    let keys = load_s3_credentials(cfg);
    let cred_count = keys.len();
    let default_readonly: std::collections::HashSet<String> = keys
        .iter()
        .filter(|(_, c)| c.readonly && c.namespace.is_none())
        .map(|(k, _)| k.clone())
        .collect();
    // 每绑定命名空间起一个服务实例（共享 Arc ✗ 仅 namespace/owner 不同）= 多租户隔离
    let mut by_key: std::collections::HashMap<String, Box<VfilesS3>> =
        std::collections::HashMap::new();
    let mut rejected_keys = std::collections::HashSet::new();
    for (access, cred) in &keys {
        let Some(slug) = &cred.namespace else {
            continue;
        };
        match namespace_repo.find_by_slug(slug).await {
            Ok(Some((ns, ns_owner))) => {
                let ro = cred.readonly;
                by_key.insert(
                    access.clone(),
                    Box::new(VfilesS3 {
                        workspace: workspace.clone(),
                        upload: upload.clone(),
                        entry_repo: entry_repo.clone(),
                        namespace: ns,
                        owner: ns_owner,
                        readonly_keys: if ro {
                            std::collections::HashSet::from([access.clone()])
                        } else {
                            std::collections::HashSet::new()
                        },
                    }),
                );
                tracing::info!(access_key = %access, slug = %slug, "S3 凭证已绑定命名空间");
            }
            Ok(None) => {
                rejected_keys.insert(access.clone());
                tracing::warn!(
                    access_key = %access,
                    slug = %slug,
                    "S3 凭证绑定的命名空间不存在，凭证已拒绝"
                );
            }
            Err(e) => {
                rejected_keys.insert(access.clone());
                tracing::warn!(
                    access_key = %access,
                    slug = %slug,
                    error = %e,
                    "S3 命名空间解析失败，凭证已拒绝"
                );
            }
        }
    }
    tracing::info!(
        credentials = cred_count,
        readonly_default = default_readonly.len(),
        bound_namespaces = by_key.len(),
        "S3 凭证表已装配"
    );
    let s3 = VfilesS3 {
        workspace,
        upload,
        entry_repo,
        namespace,
        owner,
        readonly_keys: default_readonly,
    };
    let auth = EnvAuth { keys };
    let router = vfiles_s3::S3Router {
        default_service: Box::new(s3),
        by_key,
        rejected_keys,
        expected_region: cfg.region.clone(),
    };
    let mut builder = s3s::service::S3ServiceBuilder::new(router);
    builder.set_auth(auth);
    builder.set_path_prefix("/s3");
    builder.build()
}

fn s3_http_router(service: s3s::service::S3Service) -> axum::Router {
    axum::Router::new().fallback_service(axum::error_handling::HandleError::new(
        service,
        |err: s3s::HttpError| async move {
            tracing::error!(?err, "S3 服务错误");
            axum::response::Response::builder()
                .status(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from("Internal Server Error"))
                .unwrap()
        },
    ))
}

fn spawn_s3_server(
    service: s3s::service::S3Service,
    cfg: &vfiles_config::S3Config,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let router = s3_http_router(service);
    let addr = cfg.bind_address();
    tracing::info!(addr = %addr, "S3 兼容 API 已拉起（专用端口 ✗ SigV4 静态单对认证）");
    tokio::spawn(async move {
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                let shutdown_watch = async move {
                    let _ = shutdown.changed().await;
                };
                if let Err(err) = axum::serve(listener, router)
                    .with_graceful_shutdown(shutdown_watch)
                    .await
                {
                    tracing::error!(%addr, error = %err, "S3 服务退出异常，继续提供其余服务");
                }
            }
            Err(err) => {
                // r205 式降级韧性（bind 失败不拖垮主站 ✗ 与 WebDAV 观测语义对齐）
                tracing::error!(%addr, error = %err, "S3 绑定失败（端口占用或地址非法），继续提供其余服务");
            }
        }
    });
}

#[derive(Clone)]
struct SharedS3Dispatch {
    service: s3s::service::S3Service,
    frontend: Option<FrontendAssets>,
}

async fn shared_http_s3_fallback(
    axum::Extension(dispatch): axum::Extension<SharedS3Dispatch>,
    request: axum::extract::Request,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use tower::ServiceExt;
    let signed_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("AWS4-HMAC-SHA256 "));
    let signed_query = request
        .uri()
        .query()
        .is_some_and(|q| q.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
    if signed_header || signed_query {
        return match dispatch.service.oneshot(request).await {
            Ok(response) => {
                let (parts, body) = response.into_parts();
                axum::http::Response::from_parts(parts, axum::body::Body::new(body))
            }
            Err(err) => {
                tracing::error!(?err, "共端口 S3 服务错误");
                axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        };
    }

    let Some(frontend) = dispatch.frontend else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    let accept_encoding = request
        .headers()
        .get(axum::http::header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok());
    frontend.serve(request.uri().path(), accept_encoding).await
}

/// 按配置装配 WebDAV（r110'a ✓ 用户令「默认开启」✓ config 层 auth 防御已守（r109b））。
///
/// 依赖 = FTP runtime 同族（AuthService/entry_repo/namespaces ✓ e2e 路线形明 ✓）。
#[allow(clippy::too_many_arguments)]
fn build_webdav_runtime(
    enabled: bool,
    bind: String,
    auth_service: vfiles_app::AuthService,
    namespaces: vfiles_app::NamespaceService,
    entry_repo: std::sync::Arc<dyn vfiles_domain::EntryRepo + Send + Sync>,
    workspace: std::sync::Arc<vfiles_app::DefaultWorkspaceService>,
    upload: vfiles_app::UploadService<
        vfiles_infra_sqlite::SqliteEntryRepo,
        vfiles_infra_sqlite::SqliteSnapshotRepo,
        vfiles_infra_sqlite::FsBlobStore,
        vfiles_infra_sqlite::FsUploadStore,
    >,
    audit: Option<std::sync::Arc<dyn Fn(vfiles_domain::types::NewAuditLog) + Send + Sync>>,
    embedded: bool,
    mount_path: String,
) -> Option<(
    vfiles_webdav::WebdavSettings,
    vfiles_webdav::WebdavApplication,
)> {
    if !enabled {
        return None;
    }
    let verify: vfiles_webdav::VerifyFn = {
        let auth = std::sync::Arc::new(auth_service.clone());
        std::sync::Arc::new(move |u: String, pw: String| {
            let auth = std::sync::Arc::clone(&auth);
            Box::pin(async move { auth.verify_credentials(&u, &pw).await.ok() })
        })
    };
    let app = vfiles_webdav::WebdavApplication {
        audit,
        namespaces,
        entry_repo,
        verify,
        locks: std::sync::Arc::new(vfiles_webdav::LockTable::new()),
        write: std::sync::Arc::new(WebdavWrite { workspace, upload }),
        // r-new 共端口：嵌入 = mount（/dav 等）/ 独立 = ""（现行为零回归 ✗ 1337 按此分流）
        mount_prefix: if embedded { mount_path } else { String::new() },
    };
    Some((vfiles_webdav::WebdavSettings { bind }, app))
}

/// 按配置装配 FTP 服务；未启用时返回 `None`。
fn build_ftp_runtime(
    config: &vfiles_config::AppConfig,
    deps: FtpRuntimeDeps,
) -> anyhow::Result<Option<(FtpSettings, FtpApplication)>> {
    if !config.ftp.enabled {
        return Ok(None);
    }

    if !config.auth.enabled {
        // 配置校验已覆盖；这里再做一次防御，避免出现「无认证可写」的 FTP
        bail!("启用 FTP 需要开启认证（VFILES_AUTH_ENABLED=true）");
    }

    let bind = config
        .ftp
        .bind_address()
        .parse::<std::net::SocketAddr>()
        .map_err(|err| {
            anyhow!(
                "VFILES_FTP_HOST/PORT 非法（{}）: {err}",
                config.ftp.bind_address()
            )
        })?;

    let roles = match config.ftp.allowed_roles.as_slice() {
        [] => RoleFilter::all(),
        allowed => {
            let parsed: Vec<Role> = allowed
                .iter()
                .filter_map(|value| match value.as_str() {
                    "admin" => Some(Role::Admin),
                    "manager" => Some(Role::Manager),
                    "user" => Some(Role::User),
                    other => {
                        tracing::warn!(role = other, "忽略未知的 FTP 允许角色");
                        None
                    }
                })
                .collect();
            if parsed.is_empty() {
                RoleFilter::all()
            } else {
                RoleFilter::new(parsed)
            }
        }
    };

    let snapshot_mode = match config.ftp.snapshot_mode.as_str() {
        "per-file" => vfiles_app::SnapshotMode::PerFile,
        "off" => vfiles_app::SnapshotMode::Off,
        _ => vfiles_app::SnapshotMode::Batch,
    };

    let settings = FtpSettings {
        bind,
        passive_ports: config.ftp.passive_ports,
        passive_host: config.ftp.passive_host.clone(),
        greeting: "VFiles FTP 批量导入",
        idle_timeout_secs: config.ftp.idle_timeout_seconds,
        tls_cert: config.ftp.tls_cert.clone(),
        tls_key: config.ftp.tls_key.clone(),
        tls_required: config.ftp.tls_required,
    };

    if config.ftp.tls_cert.is_none() {
        tracing::warn!(
            "FTP 未启用 TLS，凭据与数据为明文；建议仅在可信内网使用或配置 VFILES_FTP_TLS_CERT/KEY"
        );
    }

    let namespaces = vfiles_app::NamespaceService::new(Arc::clone(&deps.namespace_repo));
    let authenticator = Arc::new(VfilesAuthenticator::new(
        Arc::new(deps.auth_service),
        roles.clone(),
        deps.login_attempt_limiter,
        vfiles_app::RateLimitPolicy {
            enabled: config.auth.login_rate_limit.enabled,
            window_ms: config.auth.login_rate_limit.window_ms,
            max_attempts: config.auth.login_rate_limit.max_attempts,
        },
        Arc::clone(&deps.stats),
    ));
    let user_detail_provider = Arc::new(VfilesUserDetailProvider::new(
        deps.user_repo,
        namespaces,
        roles,
    ));

    let backend = BackendDeps {
        workspace: deps.workspace,
        entry_repo: deps.entry_repo,
        snapshot_repo: deps.snapshot_repo,
        blob_store: deps.blob_store,
        stats: deps.stats,
        max_file_size_bytes: Some(config.limits.max_file_size_bytes),
        snapshot_mode,
        flush_threshold: config.ftp.snapshot_flush_files as usize,
    };

    Ok(Some((
        settings,
        FtpApplication {
            backend,
            authenticator,
            user_detail_provider,
        },
    )))
}

/// 等待停机信号（r206 排障增强 ✗ 用户报告"连接即退出"却无法分辨信号源）：
/// - **信号名入日志**（SIGINT=Ctrl+C / SIGTERM=kill / SIGHUP=终端断开 ✗ 下次退出
///   即可分辨 → 定信号源方向 ✓）
/// - **SIGHUP 纳入优雅停机**（此前未监听 = 默认硬杀无痕 ✗✗ 现优雅 + 留痕 ✓）
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    #[cfg(unix)]
    let signal_name = {
        let mut int = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(error = %err, "failed to listen for SIGINT");
                std::future::pending().await
            }
        };
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(error = %err, "failed to listen for SIGTERM");
                std::future::pending().await
            }
        };
        let mut hangup = match signal(SignalKind::hangup()) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(error = %err, "failed to listen for SIGHUP");
                std::future::pending().await
            }
        };
        tokio::select! {
            _ = int.recv() => "SIGINT",
            _ = term.recv() => "SIGTERM",
            _ = hangup.recv() => "SIGHUP",
        }
    };

    #[cfg(not(unix))]
    let signal_name: &'static str = {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::warn!(error = %err, "failed to listen for ctrl-c");
        }
        "SIGINT"
    };

    tracing::info!(signal = %signal_name, "收到停机信号（优雅停机开始 ✗ 信号名 = 定源排障）");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn maintenance_config(interval_seconds: u64, initial_delay_seconds: u64) -> MaintenanceConfig {
        MaintenanceConfig {
            enabled: true,
            interval_seconds,
            initial_delay_seconds,
            blob_grace_seconds: 3600,
            snapshot_keep: 0,
            snapshot_max_age_days: 0,
        }
    }

    #[test]
    fn maintenance_schedule_clamps_interval_and_delay() {
        let schedule = MaintenanceSchedule::from_config(&maintenance_config(5, 10_000));

        assert_eq!(
            schedule.interval,
            Duration::from_secs(MIN_MAINTENANCE_INTERVAL_SECONDS),
            "间隔不得小于下限，避免配置成忙循环"
        );
        assert_eq!(schedule.initial_delay, Duration::from_secs(5));
    }

    #[test]
    fn maintenance_schedule_keeps_sane_values_untouched() {
        let schedule = MaintenanceSchedule::from_config(&maintenance_config(3600, 120));

        assert_eq!(schedule.interval, Duration::from_secs(3600));
        assert_eq!(schedule.initial_delay, Duration::from_secs(120));
        assert_eq!(schedule.blob_grace_seconds, 3600);
        assert_eq!(schedule.snapshot_keep, 0);
    }

    #[test]
    fn serve_accepts_host_and_port_overrides() {
        let cli = Cli::try_parse_from(["vfiles", "serve", "--host", "127.0.0.1", "--port", "8088"])
            .expect("cli should parse serve command");

        match cli.command {
            Commands::Serve(args) => {
                assert_eq!(args.host.as_deref(), Some("127.0.0.1"));
                assert_eq!(args.port, Some(8088));
            }
            other => panic!("unexpected command parsed: {other:?}"),
        }
    }

    #[test]
    fn serve_without_overrides_uses_defaults() {
        let cli = Cli::try_parse_from(["vfiles", "serve"])
            .expect("cli should parse serve command without overrides");

        match cli.command {
            Commands::Serve(args) => {
                assert_eq!(args.host, None);
                assert_eq!(args.port, None);
            }
            other => panic!("unexpected command parsed: {other:?}"),
        }
    }

    #[test]
    fn user_create_accepts_plaintext_password_argument() {
        let cli = Cli::try_parse_from([
            "vfiles",
            "user",
            "create",
            "--username",
            "admin",
            "--email",
            "admin@example.com",
            "--role",
            "admin",
            "--password",
            "secret123",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::User {
                command: UserCommands::Create(args),
            } => {
                assert_eq!(args.username, "admin");
                assert_eq!(args.email, "admin@example.com");
                assert_eq!(args.role, UserRoleArg::Admin);
                assert_eq!(args.password.as_deref(), Some("secret123"));
                assert_eq!(args.password_hash, None);
            }
            other => panic!("unexpected command parsed: {other:?}"),
        }
    }

    #[test]
    fn user_create_requires_one_password_input() {
        let err = Cli::try_parse_from([
            "vfiles",
            "user",
            "create",
            "--username",
            "admin",
            "--email",
            "admin@example.com",
        ])
        .expect_err("cli should reject missing password input");

        let message = err.to_string();
        assert!(message.contains("--password"));
        assert!(message.contains("--password-hash"));
    }

    #[test]
    fn resolve_password_hash_input_hashes_plaintext_password() {
        let hash = resolve_password_hash_input(Some("secret123"), None)
            .expect("plaintext password should hash");

        assert!(hash.starts_with("$argon2"));
        assert!(AuthService::verify_password_for_storage("secret123", &hash));
        assert!(!AuthService::verify_password_for_storage(
            "wrong-password",
            &hash
        ));
    }

    #[test]
    fn user_update_parses_enable_flag() {
        let cli = Cli::try_parse_from([
            "vfiles",
            "user",
            "update",
            "550e8400-e29b-41d4-a716-446655440000",
            "--enable",
        ])
        .expect("cli should parse update command");

        match cli.command {
            Commands::User {
                command: UserCommands::Update(args),
            } => {
                assert_eq!(args.user_id, "550e8400-e29b-41d4-a716-446655440000");
                assert!(args.enable);
                assert!(!args.disable);
            }
            other => panic!("unexpected command parsed: {other:?}"),
        }
    }

    #[test]
    fn register_systemd_parses_working_and_data_directory_arguments() {
        let cli = Cli::try_parse_from([
            "vfiles",
            "register",
            "-t",
            "systemd",
            "--service-name",
            "vfiles-prod",
            "--working-directory",
            "/srv/vfiles",
            "--data-directory",
            "/var/lib/vfiles",
            "--start",
        ])
        .expect("cli should parse register command");

        match cli.command {
            Commands::Register(args) => {
                assert_eq!(args.target, RegisterTargetArg::Systemd);
                assert_eq!(args.service_name, "vfiles-prod");
                assert_eq!(args.working_directory, Some(PathBuf::from("/srv/vfiles")));
                assert_eq!(args.data_directory, Some(PathBuf::from("/var/lib/vfiles")));
                assert!(args.start);
            }
            other => panic!("unexpected command parsed: {other:?}"),
        }
    }

    #[test]
    fn render_systemd_unit_includes_working_and_data_directory() {
        let unit = render_systemd_unit(&SystemdUnitConfig {
            service_name: "vfiles",
            executable_path: Path::new("/opt/vfiles/vfiles"),
            working_directory: Path::new("/srv/vfiles"),
            data_directory: Some(Path::new("/var/lib/vfiles")),
        });

        assert!(unit.contains("ExecStart=\"/opt/vfiles/vfiles\" serve"));
        assert!(unit.contains("WorkingDirectory=/srv/vfiles"));
        assert!(unit.contains("Environment=\"VFILES_STORAGE_ROOT=/var/lib/vfiles\""));
        assert!(unit.contains("WantedBy=multi-user.target"));
    }
}
