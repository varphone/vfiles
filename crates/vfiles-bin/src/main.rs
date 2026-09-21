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
    SqliteFavoriteRepo, SqliteHealthProbe, SqliteMigrations, SqlitePoolFactory, repo::*,
};

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

async fn run_user_update(args: UserUpdateArgs) -> anyhow::Result<()> {
    let disabled = if args.enable {
        Some(false)
    } else if args.disable {
        Some(true)
    } else {
        None
    };
    let role = args.role.map(Into::into);

    if args.email.is_none() && role.is_none() && disabled.is_none() {
        bail!("no update requested; provide --email, --role, --enable or --disable");
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
    let app = build_router(app_state);

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
    let (service_shutdown_tx, service_shutdown_rx) = tokio::sync::watch::channel(false);

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

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::warn!(error = %err, "failed to listen for ctrl-c");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received; finishing in-flight requests");
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
