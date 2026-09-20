use anyhow::{anyhow, bail};
use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use vfiles_app::{
    AdminService, AuthService, BootstrapService, HealthService, HistoryService, SearchService,
    SessionService, ShareService, UploadService, WorkspaceService,
};
use vfiles_config::ConfigLoader;
use vfiles_domain::*;
use vfiles_http::{AppState, FrontendAssets, build_router, middleware::LoginAttemptLimiter};
use vfiles_infra_fs::FsStorageBootstrap;
use vfiles_infra_sqlite::{SqliteHealthProbe, SqliteMigrations, SqlitePoolFactory, repo::*};

#[derive(Debug, Parser)]
#[command(name = "vfiles")]
#[command(about = "VFiles server")]
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
    /// Run health checks
    Check,
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
    // Initialize tracing subscriber for output
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

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
        Commands::Check => {
            run_check().await?;
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

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("client").join("dist");
            if is_frontend_dist(&candidate) {
                return Some(candidate);
            }
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
        namespace_repo: std::sync::Arc::new(namespace_repo),
        entry_repo: std::sync::Arc::new(entry_repo),
        snapshot_repo: std::sync::Arc::new(snapshot_repo),
        blob_store: Arc::new(blob_store),
        upload_store: std::sync::Arc::new(upload_store),
        login_attempt_limiter: Arc::new(LoginAttemptLimiter::new()),
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

    tracing::info!("VFiles server started successfully!");
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
