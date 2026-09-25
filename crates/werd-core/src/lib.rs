use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use uuid::Uuid;

pub mod runtimes;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

static DAEMON_START: Mutex<()> = Mutex::new(());

fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    command
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub php: String,
    pub services: Vec<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ports: Option<HashMap<String, u16>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub projects: Vec<Project>,
    pub daemon_version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DoctorResult {
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Endpoint {
    port: u16,
    token: String,
    pid: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    token: String,
    method: String,
    params: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    ok: bool,
    data: Value,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    php: String,
    services: ManifestServices,
}

#[derive(Debug, Serialize, Deserialize)]
struct ManifestServices {
    postgres: Option<PostgresManifest>,
    redis: Option<String>,
    mailpit: Option<bool>,
    rustfs: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PostgresManifest {
    major: u32,
    extensions: Vec<String>,
}

#[derive(Default)]
struct State {
    projects: Vec<Project>,
    processes: HashMap<String, Vec<ManagedChild>>,
}

struct ManagedChild {
    name: String,
    child: Child,
}

pub fn home() -> Result<PathBuf> {
    let root = if let Ok(path) = std::env::var("WERD_HOME") {
        PathBuf::from(path)
    } else {
        dirs::data_local_dir().context("Directory dati utente non trovata")?.join("Werd")
    };
    fs::create_dir_all(&root).with_context(|| format!("Impossibile creare {}", root.display()))?;
    Ok(root)
}

fn state_path(root: &Path) -> PathBuf { root.join("state.json") }
fn endpoint_path(root: &Path) -> PathBuf { root.join("daemon.json") }

fn load_state(root: &Path) -> Result<State> {
    let mut state = State::default();
    let path = state_path(root);
    if path.exists() {
        state.projects = serde_json::from_slice(&fs::read(path)?)?;
        for project in &mut state.projects {
            project.status = "stopped".into();
            project.url = None;
        }
    }
    Ok(state)
}

fn save_state(root: &Path, state: &State) -> Result<()> {
    let path = state_path(root);
    let temporary = root.join("state.json.tmp");
    let bytes = serde_json::to_vec_pretty(&state.projects)?;
    fs::write(&temporary, bytes)?;
    if path.exists() { fs::remove_file(&path)?; }
    fs::rename(temporary, path)?;
    Ok(())
}

fn default_manifest() -> Manifest {
    Manifest {
        version: 1,
        php: "8.5".into(),
        services: ManifestServices {
            postgres: Some(PostgresManifest { major: 18, extensions: vec!["pgvector".into()] }),
            redis: Some("7.2".into()),
            mailpit: Some(true),
            rustfs: Some(true),
        },
    }
}

fn manifest_services(manifest: &Manifest) -> Vec<String> {
    let mut services = Vec::new();
    if manifest.services.postgres.is_some() { services.push("postgres".into()); }
    if manifest.services.redis.is_some() { services.push("redis".into()); }
    if manifest.services.mailpit == Some(true) { services.push("mailpit".into()); }
    if manifest.services.rustfs == Some(true) { services.push("rustfs".into()); }
    services
}

fn add_project(root: &Path, state: &mut State, path: &str) -> Result<Project> {
    let canonical = fs::canonicalize(path).with_context(|| format!("Cartella non trovata: {path}"))?;
    if !canonical.is_dir() { bail!("Il percorso non è una cartella"); }
    if !canonical.join("artisan").is_file() || !canonical.join("composer.json").is_file() {
        bail!("La cartella non sembra un progetto Laravel (artisan e composer.json mancanti)");
    }
    let canonical_string = canonical.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    if state.projects.iter().any(|project| project.path == canonical_string) {
        bail!("Il progetto è già collegato a Werd");
    }
    let manifest_path = canonical.join("werd.yml");
    let manifest = if manifest_path.exists() {
        serde_yaml::from_slice::<Manifest>(&fs::read(&manifest_path)?)?
    } else {
        let manifest = default_manifest();
        fs::write(&manifest_path, serde_yaml::to_string(&manifest)?)?;
        manifest
    };
    if manifest.version != 1 { bail!("Versione werd.yml non supportata: {}", manifest.version); }
    if manifest.php != "8.5" { bail!("Per ora è supportato PHP 8.5"); }
    if let Some(postgres) = &manifest.services.postgres {
        if postgres.major != 18 || postgres.extensions != ["pgvector"] {
            bail!("Per ora è supportato PostgreSQL 18 con pgvector");
        }
    }
    if let Some(redis) = &manifest.services.redis {
        if redis != "7.2" { bail!("Per ora è supportato Redis 7.2"); }
    }
    let project = Project {
        id: Uuid::new_v4().to_string(),
        name: canonical.file_name().unwrap_or_default().to_string_lossy().to_string(),
        path: canonical_string,
        php: manifest.php.clone(),
        services: manifest_services(&manifest),
        status: "stopped".into(),
        url: None,
        error: None,
        ports: None,
    };
    state.projects.push(project.clone());
    save_state(root, state)?;
    Ok(project)
}

fn runtime_binary(root: &Path, name: &str) -> PathBuf {
    let executable = if cfg!(windows) { format!("{name}.exe") } else { name.into() };
    let family = match name {
        "php-cgi" => "php/8.5",
        "caddy" => "caddy/2.11.4",
        "postgres" | "initdb" | "psql" | "pg_ctl" => "postgres/18/bin",
        "redis-server" | "redis-cli" => "redis/7.2",
        "mailpit" => "mailpit/1.31.2",
        "rustfs" => "rustfs/1.0.0",
        _ => "unknown",
    };
    root.join("runtimes").join(family).join(executable)
}

fn free_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

fn assigned_port(ports: &mut HashMap<String, u16>, key: &str) -> Result<u16> {
    if let Some(port) = ports.get(key) { return Ok(*port); }
    for _ in 0..20 {
        let port = free_port()?;
        if !ports.values().any(|candidate| *candidate == port) {
            ports.insert(key.into(), port);
            return Ok(port);
        }
    }
    bail!("Impossibile assegnare una porta a {key}")
}

fn check_reserved_ports(ports: &HashMap<String, u16>) -> Result<()> {
    for (service, port) in ports {
        TcpListener::bind(("127.0.0.1", *port)).with_context(|| {
            format!("Porta {port} di {service} occupata. Ferma il processo che la usa o riassegna le porte del progetto")
        })?;
    }
    Ok(())
}

fn wait_for_port(port: u16, child: &mut Child, label: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() { return Ok(()); }
        if let Some(exit) = child.try_wait()? { bail!("{label} terminato all'avvio ({exit})"); }
        thread::sleep(Duration::from_millis(150));
    }
    bail!("{label} non risponde sulla porta {port}")
}

fn checked_binary(root: &Path, name: &str, label: &str) -> Result<PathBuf> {
    let path = runtime_binary(root, name);
    if !path.is_file() { bail!("Runtime {label} non installato in {}", path.display()); }
    Ok(path)
}

fn spawn_logged(root: &Path, id: &str, name: &str, mut command: Command) -> Result<ManagedChild> {
    let path = root.join("projects").join(id).join(format!("{name}.log"));
    let output = File::create(path)?;
    let child = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::from(output)).spawn()
        .with_context(|| format!("Impossibile avviare {name}"))?;
    Ok(ManagedChild { name: name.into(), child })
}

fn spawn_ready(root: &Path, id: &str, name: &str, command: Command, port: u16, label: &str) -> Result<ManagedChild> {
    let mut process = spawn_logged(root, id, name, command)?;
    if let Err(error) = wait_for_port(port, &mut process.child, label) {
        let _ = process.child.kill();
        let _ = process.child.wait();
        return Err(error);
    }
    Ok(process)
}

fn postgres_password(directory: &Path) -> Result<String> {
    let path = directory.join("postgres-password.txt");
    if path.exists() { return Ok(fs::read_to_string(path)?.trim().into()); }
    let password = Uuid::new_v4().simple().to_string();
    fs::write(path, format!("{password}\n"))?;
    Ok(password)
}

fn rustfs_credentials(directory: &Path) -> Result<(String, String)> {
    let path = directory.join("rustfs-credentials.json");
    if path.exists() {
        let value: Value = serde_json::from_slice(&fs::read(path)?)?;
        return Ok((value["access_key"].as_str().context("Access key RustFS mancante")?.into(), value["secret_key"].as_str().context("Secret key RustFS mancante")?.into()));
    }
    let access = format!("werd{}", Uuid::new_v4().simple());
    let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    fs::write(path, serde_json::to_vec(&json!({ "access_key": access, "secret_key": secret }))?)?;
    Ok((access, secret))
}

fn launch_services(root: &Path, project: &Project, children: &mut Vec<ManagedChild>, ports: &mut HashMap<String, u16>) -> Result<()> {
    let directory = root.join("projects").join(&project.id);
    if project.services.iter().any(|name| name == "postgres") {
        let postgres = checked_binary(root, "postgres", "PostgreSQL 18")?;
        let initdb = checked_binary(root, "initdb", "PostgreSQL initdb")?;
        let psql = checked_binary(root, "psql", "PostgreSQL psql")?;
        let pg_dir = directory.join("postgres");
        let password = postgres_password(&directory)?;
        if !pg_dir.join("PG_VERSION").exists() {
            let password_file = directory.join("postgres-password.txt");
            let output = hidden_command(initdb).arg("-D").arg(&pg_dir).arg("-U").arg("werd")
                .arg("--auth-host=scram-sha-256").arg("--auth-local=trust")
                .arg("--pwfile").arg(password_file).output().context("initdb non avviato")?;
            if !output.status.success() { bail!("initdb fallito: {}", String::from_utf8_lossy(&output.stderr)); }
        }
        let vector_control = postgres.parent().context("Percorso PostgreSQL non valido")?.parent()
            .context("Percorso PostgreSQL non valido")?.join("share").join("extension").join("vector.control");
        let vector_library = postgres.parent().context("Percorso PostgreSQL non valido")?.parent()
            .context("Percorso PostgreSQL non valido")?.join("lib").join(if cfg!(windows) { "vector.dll" } else { "vector.so" });
        if !vector_control.is_file() || !vector_library.is_file() { bail!("pgvector non installato nel runtime PostgreSQL 18"); }
        let port = assigned_port(ports, "postgres")?;
        let mut command = hidden_command(postgres);
        command.arg("-D").arg(&pg_dir).arg("-h").arg("127.0.0.1").arg("-p").arg(port.to_string());
        children.push(spawn_ready(root, &project.id, "postgres", command, port, "PostgreSQL")?);
        let output = hidden_command(&psql).arg("-h").arg("127.0.0.1").arg("-p").arg(port.to_string())
            .arg("-U").arg("werd").arg("-d").arg("postgres")
            .arg("-tc").arg("SELECT 1 FROM pg_database WHERE datname='app'")
            .env("PGPASSWORD", &password).output()?;
        if !output.status.success() { bail!("Controllo database PostgreSQL fallito"); }
        if String::from_utf8_lossy(&output.stdout).trim() != "1" {
            let output = hidden_command(&psql).arg("-h").arg("127.0.0.1").arg("-p").arg(port.to_string())
                .arg("-U").arg("werd").arg("-d").arg("postgres").arg("-c").arg("CREATE DATABASE app")
                .env("PGPASSWORD", &password).output()?;
            if !output.status.success() { bail!("Creazione database app fallita: {}", String::from_utf8_lossy(&output.stderr)); }
        }
        let output = hidden_command(&psql).arg("-h").arg("127.0.0.1").arg("-p").arg(port.to_string())
            .arg("-U").arg("werd").arg("-d").arg("app").arg("-c").arg("CREATE EXTENSION IF NOT EXISTS vector")
            .env("PGPASSWORD", &password).output()?;
        if !output.status.success() { bail!("Attivazione pgvector fallita: {}", String::from_utf8_lossy(&output.stderr)); }
    }
    if project.services.iter().any(|name| name == "redis") {
        let binary = checked_binary(root, "redis-server", "Redis 7.2")?;
        let data = directory.join("redis");
        fs::create_dir_all(&data)?;
        let port = assigned_port(ports, "redis")?;
        let mut command = hidden_command(binary);
        command.arg("--bind").arg("127.0.0.1").arg("--port").arg(port.to_string())
            .arg("--dir").arg(&data).arg("--appendonly").arg("yes");
        children.push(spawn_ready(root, &project.id, "redis", command, port, "Redis")?);
    }
    if project.services.iter().any(|name| name == "mailpit") {
        let binary = checked_binary(root, "mailpit", "Mailpit")?;
        let smtp = assigned_port(ports, "mailpit_smtp")?;
        let ui = assigned_port(ports, "mailpit_ui")?;
        let mut command = hidden_command(binary);
        command.arg("--smtp").arg(format!("127.0.0.1:{smtp}"))
            .arg("--listen").arg(format!("127.0.0.1:{ui}"))
            .arg("--database").arg(directory.join("mailpit.db"))
            .arg("--disable-version-check");
        children.push(spawn_ready(root, &project.id, "mailpit", command, smtp, "Mailpit SMTP")?);
    }
    if project.services.iter().any(|name| name == "rustfs") {
        let binary = checked_binary(root, "rustfs", "RustFS")?;
        let data = directory.join("rustfs");
        fs::create_dir_all(&data)?;
        let (access, secret) = rustfs_credentials(&directory)?;
        let api = assigned_port(ports, "rustfs_api")?;
        let console = assigned_port(ports, "rustfs_console")?;
        let mut command = hidden_command(binary);
        command.arg("server").arg("--address").arg(format!("127.0.0.1:{api}"))
            .arg("--console-enable").arg("--console-address").arg(format!("127.0.0.1:{console}"))
            .arg(&data).env("RUSTFS_ACCESS_KEY", access).env("RUSTFS_SECRET_KEY", secret);
        children.push(spawn_ready(root, &project.id, "rustfs", command, api, "RustFS")?);
    }
    Ok(())
}

fn append_log(root: &Path, id: &str, message: &str) -> Result<()> {
    let directory = root.join("projects").join(id);
    fs::create_dir_all(&directory)?;
    let mut file = OpenOptions::new().create(true).append(true).open(directory.join("werd.log"))?;
    writeln!(file, "{message}")?;
    Ok(())
}

fn terminate_children(root: &Path, children: &mut Vec<ManagedChild>, ports: &HashMap<String, u16>, id: &str) {
    if children.iter().any(|process| process.name == "postgres") {
        let binary = runtime_binary(root, "pg_ctl");
        let data = root.join("projects").join(id).join("postgres");
        let _ = hidden_command(binary).arg("-D").arg(data).arg("stop").arg("-m").arg("fast").arg("-w").status();
    }
    if children.iter().any(|process| process.name == "redis") {
        if let Some(port) = ports.get("redis") {
            let _ = hidden_command(runtime_binary(root, "redis-cli")).arg("-h").arg("127.0.0.1")
                .arg("-p").arg(port.to_string()).arg("SHUTDOWN").status();
        }
    }
    for process in children.iter_mut().rev() {
        if process.child.try_wait().ok().flatten().is_none() { let _ = process.child.kill(); }
        let _ = process.child.wait();
    }
    children.clear();
}

fn start_project(root: &Path, state: &mut State, id: &str) -> Result<Project> {
    let index = state.projects.iter().position(|project| project.id == id).context("Progetto non trovato")?;
    if state.projects[index].status == "running" { return Ok(state.projects[index].clone()); }
    let project = state.projects[index].clone();
    let php = checked_binary(root, "php-cgi", "PHP 8.5")?;
    let caddy = checked_binary(root, "caddy", "Caddy")?;
    let mut ports = project.ports.clone().unwrap_or_default();
    check_reserved_ports(&ports)?;
    let site_port = assigned_port(&mut ports, "site")?;
    let fastcgi_port = assigned_port(&mut ports, "fastcgi")?;
    let data_dir = root.join("projects").join(id);
    fs::create_dir_all(&data_dir)?;
    let public_dir = Path::new(&project.path).join("public");
    if !public_dir.join("index.php").is_file() { bail!("public/index.php non trovato"); }
    let mut children = Vec::new();
    if let Err(error) = launch_services(root, &project, &mut children, &mut ports) {
        terminate_children(root, &mut children, &ports, id);
        return Err(error);
    }
    let caddyfile = data_dir.join("Caddyfile");
    let quoted_root = public_dir.to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/").replace('"', "\\\"");
    let caddy_config = format!("{{\n  admin off\n  auto_https disable_redirects\n  skip_install_trust\n}}\nhttps://localhost:{site_port} {{\n  bind 127.0.0.1\n  root * \"{quoted_root}\"\n  php_fastcgi 127.0.0.1:{fastcgi_port}\n  file_server\n  tls internal\n}}\n");
    if let Err(error) = fs::write(&caddyfile, caddy_config) {
        terminate_children(root, &mut children, &ports, id);
        return Err(error.into());
    }
    let mut php_command = hidden_command(php);
    php_command.arg("-b").arg(format!("127.0.0.1:{fastcgi_port}")).current_dir(&project.path);
    let mut php_process = match spawn_logged(root, id, "php", php_command) {
        Ok(process) => process,
        Err(error) => { terminate_children(root, &mut children, &ports, id); return Err(error); }
    };
    if let Err(error) = wait_for_port(fastcgi_port, &mut php_process.child, "PHP FastCGI") {
        children.push(php_process);
        terminate_children(root, &mut children, &ports, id);
        return Err(error);
    }
    children.push(php_process);
    let mut caddy_command = hidden_command(caddy);
    caddy_command.arg("run").arg("--config").arg(&caddyfile).arg("--adapter").arg("caddyfile")
        .current_dir(&data_dir).env("XDG_DATA_HOME", root.join("caddy-data"));
    let mut caddy_process = match spawn_logged(root, id, "caddy", caddy_command) {
        Ok(process) => process,
        Err(error) => { terminate_children(root, &mut children, &ports, id); return Err(error); }
    };
    if let Err(error) = wait_for_port(site_port, &mut caddy_process.child, "Caddy HTTPS") {
        children.push(caddy_process);
        terminate_children(root, &mut children, &ports, id);
        return Err(error);
    }
    children.push(caddy_process);
    state.processes.insert(id.into(), children);
    state.projects[index].status = "running".into();
    state.projects[index].error = None;
    state.projects[index].url = Some(format!("https://localhost:{site_port}"));
    state.projects[index].ports = Some(ports);
    append_log(root, id, &format!("Sito avviato: https://localhost:{site_port}"))?;
    save_state(root, state)?;
    Ok(state.projects[index].clone())
}

fn stop_project(root: &Path, state: &mut State, id: &str) -> Result<Project> {
    let index = state.projects.iter().position(|project| project.id == id).context("Progetto non trovato")?;
    if let Some(mut children) = state.processes.remove(id) {
        let ports = state.projects[index].ports.clone().unwrap_or_default();
        terminate_children(root, &mut children, &ports, id);
    }
    state.projects[index].status = "stopped".into();
    state.projects[index].url = None;
    append_log(root, id, "Progetto fermato")?;
    save_state(root, state)?;
    Ok(state.projects[index].clone())
}

fn reset_ports(root: &Path, state: &mut State, id: &str) -> Result<Project> {
    let project = state.projects.iter_mut().find(|project| project.id == id).context("Progetto non trovato")?;
    if project.status == "running" { bail!("Ferma prima il progetto"); }
    project.ports = None;
    project.error = None;
    project.status = "stopped".into();
    let updated = project.clone();
    append_log(root, id, "Porte riassegnabili al prossimo avvio; aggiorna il file .env")?;
    save_state(root, state)?;
    Ok(updated)
}

fn open_site(state: &State, id: &str) -> Result<String> {
    let project = state.projects.iter().find(|project| project.id == id).context("Progetto non trovato")?;
    let url = project.url.as_deref().context("Avvia prima il progetto")?;
    let port = url.strip_prefix("https://localhost:").context("URL del progetto non valido")?;
    port.parse::<u16>().context("Porta del sito non valida")?;
    if cfg!(windows) {
        hidden_command("rundll32.exe").arg("url.dll,FileProtocolHandler").arg(url).spawn()
            .context("Impossibile aprire il browser")?;
    } else if cfg!(target_os = "macos") {
        hidden_command("open").arg(url).spawn().context("Impossibile aprire il browser")?;
    } else { bail!("Apertura browser non disponibile su questa piattaforma"); }
    Ok(url.into())
}

fn logs(root: &Path, id: &str, service: &str) -> Result<Vec<String>> {
    if !["werd", "postgres", "redis", "mailpit", "rustfs", "php", "caddy"].contains(&service) {
        bail!("Log sconosciuto: {service}");
    }
    let path = root.join("projects").join(id).join(format!("{service}.log"));
    if !path.exists() { return Ok(Vec::new()); }
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > 256 * 1024 { file.seek(SeekFrom::Start(size - 256 * 1024))?; }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if size > 256 * 1024 {
        if let Some(end) = bytes.iter().position(|byte| *byte == b'\n') { bytes.drain(..=end); }
    }
    let content = match String::from_utf8(bytes.clone()) {
        Ok(text) => text,
        Err(_) if cfg!(windows) => encoding_rs::WINDOWS_1252.decode(&bytes).0.into_owned(),
        Err(_) => String::from_utf8_lossy(&bytes).into_owned(),
    };
    let lines = content.lines().map(str::to_string).collect::<Vec<_>>();
    Ok(lines.into_iter().rev().take(100).collect::<Vec<_>>().into_iter().rev().collect())
}

fn doctor(root: &Path) -> Vec<DoctorResult> {
    let mut checks = vec![DoctorResult { label: "Gestore Werd".into(), ok: true, detail: "Il gestore locale risponde".into() }];
    for port in [80u16, 443] {
        let available = TcpListener::bind(("127.0.0.1", port)).is_ok();
        checks.push(DoctorResult { label: format!("Porta web {port}"), ok: true,
            detail: if available { "Libera; Werd usa comunque una porta HTTPS dedicata".into() }
                else { "Occupata da un altro processo; Werd usa una porta HTTPS dedicata senza modificarlo".into() } });
    }
    for (binary, label) in [("php-cgi", "PHP 8.5"), ("caddy", "Caddy"), ("postgres", "PostgreSQL 18"), ("redis-server", "Redis 7.2"), ("mailpit", "Mailpit"), ("rustfs", "RustFS")] {
        let path = runtime_binary(root, binary);
        let ok = path.is_file();
        checks.push(DoctorResult { label: label.into(), ok, detail: if ok { format!("Disponibile: {}", path.display()) } else { format!("Non installato: {}", path.display()) } });
    }
    let vector = root.join("runtimes/postgres/18/share/extension/vector.control");
    let vector_library = root.join("runtimes/postgres/18/lib/vector.dll");
    let vector_ok = vector.is_file() && vector_library.is_file();
    checks.push(DoctorResult { label: "pgvector".into(), ok: vector_ok, detail: if vector_ok { "Estensione presente per PostgreSQL 18".into() } else { "Installa pgvector dalla pagina Diagnostica dopo PostgreSQL 18".into() } });
    checks
}

fn trust_local_ca(root: &Path) -> Result<String> {
    let certificate = root.join("caddy-data/caddy/pki/authorities/local/root.crt");
    if !certificate.is_file() { bail!("Avvia un sito Werd prima di installare il certificato locale"); }
    if cfg!(windows) {
        let output = hidden_command("certutil.exe").arg("-user").arg("-addstore")
            .arg("Root").arg(&certificate).output().context("certutil non disponibile")?;
        if !output.status.success() {
            bail!("Installazione certificato fallita: {}", String::from_utf8_lossy(&output.stderr));
        }
        Ok("CA locale Werd aggiunta alle radici attendibili dell'utente corrente".into())
    } else {
        bail!("Installazione CA non ancora disponibile su questa piattaforma")
    }
}

fn env_vars(root: &Path, project: &Project) -> Result<String> {
    let ports = project.ports.as_ref().context("Avvia prima il progetto per conoscere le porte")?;
    let directory = root.join("projects").join(&project.id);
    let mut lines = Vec::new();
    if let Some(port) = ports.get("postgres") {
        lines.extend(["DB_CONNECTION=pgsql".to_string(), "DB_HOST=127.0.0.1".to_string(), format!("DB_PORT={port}"), "DB_DATABASE=app".to_string(), "DB_USERNAME=werd".to_string(), format!("DB_PASSWORD={}", postgres_password(&directory)?)]);
    }
    if let Some(port) = ports.get("redis") { lines.extend(["REDIS_HOST=127.0.0.1".to_string(), format!("REDIS_PORT={port}"), "REDIS_PASSWORD=null".to_string()]); }
    if let Some(port) = ports.get("mailpit_smtp") { lines.extend(["MAIL_MAILER=smtp".to_string(), "MAIL_HOST=127.0.0.1".to_string(), format!("MAIL_PORT={port}"), "MAIL_USERNAME=null".to_string(), "MAIL_PASSWORD=null".to_string(), "MAIL_ENCRYPTION=null".to_string()]); }
    if let Some(port) = ports.get("rustfs_api") {
        let (access, secret) = rustfs_credentials(&directory)?;
        lines.extend(["FILESYSTEM_DISK=s3".to_string(), format!("AWS_ACCESS_KEY_ID={access}"), format!("AWS_SECRET_ACCESS_KEY={secret}"), "AWS_DEFAULT_REGION=us-east-1".to_string(), "AWS_BUCKET=werd".to_string(), format!("AWS_ENDPOINT=http://127.0.0.1:{port}"), "AWS_USE_PATH_STYLE_ENDPOINT=true".to_string()]);
    }
    Ok(lines.join("\n"))
}

fn handle(root: &Path, state: &mut State, request: Request, token: &str) -> Result<Value> {
    if request.token != token { bail!("Token locale non valido"); }
    match request.method.as_str() {
        "ping" => Ok(json!({ "version": VERSION })),
        "list" => Ok(serde_json::to_value(Snapshot { projects: state.projects.clone(), daemon_version: VERSION.into() })?),
        "add" => Ok(serde_json::to_value(add_project(root, state, request.params["path"].as_str().context("Percorso mancante")?)?)?),
        "start" => {
            let id = request.params["id"].as_str().context("ID mancante")?;
            match start_project(root, state, id) {
                Ok(project) => Ok(serde_json::to_value(project)?),
                Err(error) => {
                    if let Some(project) = state.projects.iter_mut().find(|project| project.id == id) {
                        project.status = "error".into();
                        project.error = Some(format!("{error:#}"));
                    }
                    let _ = append_log(root, id, &format!("Avvio fallito: {error:#}"));
                    let _ = save_state(root, state);
                    Err(error)
                }
            }
        },
        "stop" => Ok(serde_json::to_value(stop_project(root, state, request.params["id"].as_str().context("ID mancante")?)?)?),
        "reset-ports" => Ok(serde_json::to_value(reset_ports(root, state, request.params["id"].as_str().context("ID mancante")?)?)?),
        "open" => Ok(json!(open_site(state, request.params["id"].as_str().context("ID mancante")?)?)),
        "logs" => {
            let id = request.params["id"].as_str().context("ID mancante")?;
            if !state.projects.iter().any(|project| project.id == id) { bail!("Progetto non trovato"); }
            Ok(serde_json::to_value(logs(root, id, request.params["service"].as_str().unwrap_or("werd"))?)?)
        },
        "doctor" => Ok(serde_json::to_value(doctor(root))?),
        "trust-ca" => Ok(json!(trust_local_ca(root)?)),
        "runtimes" => Ok(serde_json::to_value(runtimes::list(root))?),
        "install" => Ok(serde_json::to_value(runtimes::install(root, request.params["id"].as_str().context("Runtime mancante")?)?)?),
        "env" => {
            let id = request.params["id"].as_str().context("ID mancante")?;
            let project = state.projects.iter().find(|project| project.id == id).context("Progetto non trovato")?;
            Ok(json!(env_vars(root, project)?))
        },
        other => bail!("Metodo sconosciuto: {other}"),
    }
}

fn serve_connection(root: PathBuf, shared: Arc<Mutex<State>>, token: String, mut stream: TcpStream) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(650)))?;
    stream.set_write_timeout(Some(Duration::from_secs(650)))?;
    let mut line = String::new();
    BufReader::new(stream.try_clone()?).read_line(&mut line)?;
    if line.len() > 1_000_000 { bail!("Richiesta troppo grande"); }
    let response = match serde_json::from_str::<Request>(&line) {
        Ok(request) => {
            let mut guard = shared.lock().map_err(|_| anyhow!("Stato non disponibile"))?;
            let result = handle(&root, &mut guard, request, &token);
            match result {
                Ok(data) => Response { ok: true, data, error: None },
                Err(error) => Response { ok: false, data: Value::Null, error: Some(format!("{error:#}")) },
            }
        }
        Err(error) => Response { ok: false, data: Value::Null, error: Some(format!("Richiesta non valida: {error}")) },
    };
    serde_json::to_writer(&mut stream, &response)?;
    writeln!(stream)?;
    Ok(())
}

pub fn run_daemon() -> Result<()> {
    let root = home()?;
    if rpc("ping", json!({})).is_ok() { bail!("Un gestore Werd è già attivo"); }
    let state = Arc::new(Mutex::new(load_state(&root)?));
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = Endpoint { port: listener.local_addr()?.port(), token: Uuid::new_v4().to_string(), pid: std::process::id() };
    fs::write(endpoint_path(&root), serde_json::to_vec(&endpoint)?)?;
    let monitor = Arc::clone(&state);
    let monitor_root = root.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(2));
        if let Ok(mut state) = monitor.lock() {
            let mut failed = Vec::new();
            for (id, children) in &mut state.processes {
                for process in children {
                    if let Ok(Some(exit)) = process.child.try_wait() {
                        failed.push((id.clone(), process.name.clone(), exit.to_string()));
                    }
                }
            }
            for (id, name, exit) in failed {
                let ports = state.projects.iter().find(|project| project.id == id)
                    .and_then(|project| project.ports.clone()).unwrap_or_default();
                if let Some(mut children) = state.processes.remove(&id) {
                    terminate_children(&monitor_root, &mut children, &ports, &id);
                }
                if let Some(project) = state.projects.iter_mut().find(|project| project.id == id) {
                    project.status = "error".into();
                    project.error = Some(format!("Il processo {name} è terminato: {exit}"));
                    project.url = None;
                }
                let _ = append_log(&monitor_root, &id, &format!("Errore: {name} terminato: {exit}"));
                let _ = save_state(&monitor_root, &state);
            }
        }
    });
    for connection in listener.incoming() {
        if let Ok(stream) = connection {
            let root = root.clone();
            let shared = Arc::clone(&state);
            let token = endpoint.token.clone();
            thread::spawn(move || { let _ = serve_connection(root, shared, token, stream); });
        }
    }
    Ok(())
}

pub fn rpc(method: &str, params: Value) -> Result<Value> {
    let endpoint: Endpoint = serde_json::from_slice(&fs::read(endpoint_path(&home()?)).context("Gestore Werd non avviato")?)?;
    let mut stream = TcpStream::connect_timeout(&format!("127.0.0.1:{}", endpoint.port).parse()?, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(650)))?;
    let request = Request { token: endpoint.token, method: method.into(), params };
    serde_json::to_writer(&mut stream, &request)?;
    writeln!(stream)?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    let response: Response = serde_json::from_str(&line)?;
    if response.ok { Ok(response.data) } else { bail!("{}", response.error.unwrap_or_else(|| "Errore sconosciuto".into())) }
}

pub fn ensure_daemon(executable: &Path) -> Result<()> {
    if rpc("ping", json!({})).is_ok() { return Ok(()); }
    let _guard = DAEMON_START.lock().map_err(|_| anyhow!("Avvio del gestore non disponibile"))?;
    if rpc("ping", json!({})).is_ok() { return Ok(()); }
    let log = File::create(home()?.join("daemon-startup.log"))?;
    let mut child = hidden_command(executable).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::from(log)).spawn()
        .with_context(|| format!("Impossibile avviare {}", executable.display()))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if rpc("ping", json!({})).is_ok() { return Ok(()); }
        if let Some(exit) = child.try_wait()? {
            let details = fs::read_to_string(home()?.join("daemon-startup.log")).unwrap_or_default();
            bail!("Il gestore Werd si è chiuso all'avvio ({exit}): {details}");
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("Il gestore Werd non risponde dopo 30 secondi; controlla daemon-startup.log nella cartella dati Werd")
}

pub fn daemon_executable() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("WERD_DAEMON_BIN") { return Ok(PathBuf::from(path)); }
    let current = std::env::current_exe()?;
    let name = if cfg!(windows) { "werd-daemon.exe" } else { "werd-daemon" };
    let adjacent = current.parent().context("Percorso eseguibile non valido")?.join(name);
    if adjacent.exists() { return Ok(adjacent); }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug").join(name);
    if dev.exists() { return Ok(dev); }
    bail!("werd-daemon non trovato; compila il binario o imposta WERD_DAEMON_BIN")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_manifest_matches_first_release() {
        let manifest = default_manifest();
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.php, "8.5");
        assert_eq!(manifest_services(&manifest), ["postgres", "redis", "mailpit", "rustfs"]);
        let decoded: Manifest = serde_yaml::from_str(&serde_yaml::to_string(&manifest).unwrap()).unwrap();
        assert_eq!(decoded.services.postgres.unwrap().major, 18);
    }
}
