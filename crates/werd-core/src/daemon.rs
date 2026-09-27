//! The `werd-daemon` process: owns the state, supervises children, serves RPC.

use crate::catalog::Catalog;
use crate::instances::{self, CreateRequest};
use crate::jobs::Jobs;
use crate::model::{ProjectStatus, Snapshot};
use crate::paths::home;
use crate::process::append_log;
use crate::rpc::{self, PROTOCOL_VERSION};
use crate::runtimes::{self, Fetcher, HttpFetcher, Installed};
use crate::settings::Settings;
use crate::state::State;
use crate::{doctor, migrations, process, projects, shims, VERSION};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MONITOR_INTERVAL: Duration = Duration::from_secs(2);
/// Parked folders are rescanned every few monitor ticks.
const PARKS_SCAN_TICKS: u64 = 3;

/// Everything a request needs besides the locked project state.
#[derive(Clone)]
pub(crate) struct Daemon {
    pub root: PathBuf,
    pub jobs: Jobs,
    pub fetcher: Arc<dyn Fetcher>,
    /// Fixed catalog for tests; the embedded or cached catalog otherwise.
    pub catalog: Option<Arc<Catalog>>,
    /// Shared state, for background jobs that must act once a download ends.
    pub state: Arc<Mutex<State>>,
}

impl Daemon {
    fn catalog(&self) -> Catalog {
        self.catalog
            .as_deref()
            .cloned()
            .unwrap_or_else(|| Catalog::load(&self.root))
    }
}

fn text<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params[key].as_str().with_context(|| format!("Missing {key}"))
}

fn id(params: &Value) -> Result<&str> {
    text(params, "id")
}

/// Starts an install or update job after checking that nothing running uses the line.
fn start_install(daemon: &Daemon, state: &State, params: &Value, update: bool) -> Result<Value> {
    let product = text(params, "product")?.to_string();
    let line = text(params, "line")?.to_string();
    let catalog = daemon.catalog();
    catalog.build(&product, &line)?;
    let installed = Installed::load(&daemon.root)?.version(&product, &line).is_some();
    if update && !installed {
        bail!("{product} {line} is not installed");
    }
    if installed {
        let users = runtime_users(state, &product, &line);
        if !users.is_empty() {
            bail!("Stop {} before updating {product} {line}", users.join(", "));
        }
    }
    let (root, fetcher) = (daemon.root.clone(), Arc::clone(&daemon.fetcher));
    let (job_product, job_line) = (product.clone(), line.clone());
    let job = daemon.jobs.start(
        &product,
        &line,
        if update { "update" } else { "install" },
        move |progress| {
            runtimes::install(
                &root,
                &catalog,
                fetcher.as_ref(),
                &job_product,
                &job_line,
                progress,
            )
            .map(|_| ())
        },
    )?;
    Ok(json!(job))
}

/// Running sites and service instances that use a runtime line.
fn runtime_users(state: &State, product: &str, line: &str) -> Vec<String> {
    let parent = if matches!(product, "phpredis" | "phpmongodb") {
        "php"
    } else {
        product
    };
    let mut users = projects::using_runtime(state, parent, line);
    users.extend(instances::using_runtime(&state.instances, product, line));
    users
}

/// Creates a service instance. When its runtime (or pgvector) is missing, a
/// background job installs it first; the instance then starts if requested.
fn create_service(daemon: &Daemon, state: &mut State, params: &Value) -> Result<Value> {
    let start_after = params["start"].as_bool().unwrap_or(true);
    let request: CreateRequest = serde_json::from_value(params.clone()).context("Invalid service request")?;
    create_instance(daemon, state, request, start_after)
}

/// Creates the instances a site still needs and links them; returns the install jobs.
fn resolve_site(daemon: &Daemon, state: &mut State, id: &str) -> Result<Value> {
    let catalog = daemon.catalog();
    let mut jobs = Vec::new();
    for requirement in projects::pending(state, id)? {
        let product = catalog.product(&requirement.product)?;
        let line = match &requirement.line {
            Some(line) => line.clone(),
            None => product
                .available_lines()
                .first()
                .map(|(line, _)| (*line).clone())
                .with_context(|| format!("{} is not available for this platform", product.label))?,
        };
        let request = CreateRequest {
            product: requirement.product.clone(),
            line,
            name: None,
            port: None,
            autostart: true,
            extensions: requirement.extensions.clone(),
        };
        let created = create_instance(daemon, state, request, true)?;
        let instance = created["instance"]["id"]
            .as_str()
            .context("Missing instance id")?
            .to_string();
        projects::link(&daemon.root, state, id, &requirement.category, &instance, None)?;
        if !created["job"].is_null() {
            jobs.push(created["job"].clone());
        }
    }
    Ok(json!({ "project": state.project(id)?, "jobs": jobs }))
}

fn create_instance(
    daemon: &Daemon,
    state: &mut State,
    request: CreateRequest,
    start_after: bool,
) -> Result<Value> {
    let root = daemon.root.clone();
    let catalog = daemon.catalog();
    let instance = instances::create(&root, &mut state.instances, &catalog, request)?;

    let installed = Installed::load(&root)?;
    let needs_runtime = installed.version(&instance.product, &instance.line).is_none();
    let wants_vector = instance.extensions.iter().any(|name| name == "pgvector");
    let needs_vector = wants_vector
        && !runtimes::has_pgvector(&runtimes::line_dir(&root, &instance.product, &instance.line))
        && installed.lines("pgvector").is_empty();
    if !needs_runtime && !needs_vector {
        let instance = if start_after {
            instances::start(&root, &mut state.instances, &instance.id)?
        } else {
            instance
        };
        return Ok(json!({ "instance": instance, "job": null }));
    }

    let vector_line = catalog
        .product("pgvector")
        .ok()
        .and_then(|product| product.available_lines().first().map(|(line, _)| (*line).clone()));
    let (fetcher, shared, id) = (
        Arc::clone(&daemon.fetcher),
        Arc::clone(&daemon.state),
        instance.id.clone(),
    );
    let (product, line) = (instance.product.clone(), instance.line.clone());
    let job = daemon
        .jobs
        .start(&instance.product, &instance.line, "install", move |progress| {
            if needs_runtime {
                runtimes::install(&root, &catalog, fetcher.as_ref(), &product, &line, progress)?;
            }
            if needs_vector {
                let vector_line = vector_line.context("pgvector is not available for this platform")?;
                runtimes::install(
                    &root,
                    &catalog,
                    fetcher.as_ref(),
                    "pgvector",
                    &vector_line,
                    progress,
                )?;
            }
            if start_after {
                progress.step("Starting");
                let mut state = shared.lock().map_err(|_| anyhow!("Daemon state unavailable"))?;
                instances::start(&root, &mut state.instances, &id)?;
            }
            Ok(())
        })?;
    Ok(json!({ "instance": instance, "job": job }))
}

/// Starts a site when Caddy, the web server every site shares, is not installed
/// yet: a background job downloads it and then starts the site. The site is
/// reported as starting meanwhile, and as failed if either step fails.
fn start_after_caddy(daemon: &Daemon, state: &mut State, id: &str) -> Result<Value> {
    let jobs = daemon.jobs.clone();
    let catalog = daemon.catalog();
    let line = catalog
        .product("caddy")?
        .available_lines()
        .first()
        .map(|(line, _)| (*line).clone())
        .context("Caddy is not available for this platform")?;
    let index = state.index(id)?;
    let (root, fetcher, shared, site) = (
        daemon.root.clone(),
        Arc::clone(&daemon.fetcher),
        Arc::clone(&daemon.state),
        id.to_string(),
    );
    daemon
        .jobs
        .start("caddy", &line.clone(), "install", move |progress| {
            wait_for_setup(&jobs);
            let installed = if runtimes::resolve_line(&root, "caddy", None, "Caddy").is_ok() {
                Ok(Default::default())
            } else {
                runtimes::install(&root, &catalog, fetcher.as_ref(), "caddy", &line, progress)
            };
            progress.step("Starting the site");
            let mut state = shared.lock().map_err(|_| anyhow!("Daemon state unavailable"))?;
            match installed.and_then(|_| projects::start(&root, &mut state, &site)) {
                Ok(_) => Ok(()),
                Err(error) => {
                    projects::mark_failed(&root, &mut state, &site, &error);
                    Err(error)
                }
            }
        })?;
    let project = &mut state.projects[index];
    project.status = ProjectStatus::Starting;
    project.error = None;
    Ok(json!(project.clone()))
}

/// Product of the first-run setup job, as shown in `jobs.list`.
const SETUP: &str = "setup";

/// What a fresh Werd needs before the first site: Caddy, the newest PHP line
/// (made the global version) and Composer. Returns the missing ones.
fn setup_missing(root: &Path, catalog: &Catalog) -> Vec<(String, String)> {
    let installed = Installed::load(root).unwrap_or_default();
    ["caddy", "php", "composer"]
        .iter()
        .filter(|product| installed.lines(product).is_empty())
        .filter_map(|product| {
            let line = catalog
                .product(product)
                .ok()?
                .available_lines()
                .first()?
                .0
                .clone();
            Some((product.to_string(), line))
        })
        .collect()
}

/// Starts the first-run setup in the background. Without `force` it runs only
/// until it has succeeded once, so removing PHP later does not bring it back.
fn start_setup(daemon: &Daemon, force: bool) -> Result<Option<crate::jobs::Job>> {
    let root = daemon.root.clone();
    let mut settings = Settings::load(&root)?;
    if settings.setup_done && !force {
        return Ok(None);
    }
    let catalog = daemon.catalog();
    let missing = setup_missing(&root, &catalog);
    if missing.is_empty() {
        settings.setup_done = true;
        settings.save(&root)?;
        return Ok(None);
    }
    let fetcher = Arc::clone(&daemon.fetcher);
    let job = daemon.jobs.start(SETUP, "werd", SETUP, move |progress| {
        for (product, line) in missing {
            // A site start may have installed Caddy meanwhile.
            if Installed::load(&root)?.version(&product, &line).is_some() {
                continue;
            }
            let label = catalog
                .product(&product)
                .map(|p| p.label.clone())
                .unwrap_or_else(|_| product.clone());
            progress.step(&format!("Downloading {label} {line}"));
            progress.bytes(0, None);
            runtimes::install(&root, &catalog, fetcher.as_ref(), &product, &line, progress)?;
            if product == "php" {
                let mut settings = Settings::load(&root)?;
                if settings.default_php.is_none() {
                    settings.default_php = Some(line.clone());
                    settings.save(&root)?;
                }
            }
        }
        let mut settings = Settings::load(&root)?;
        settings.setup_done = true;
        settings.save(&root)
    })?;
    Ok(Some(job))
}

/// Waits for a running first-run setup, so two jobs never install Caddy at once.
fn wait_for_setup(jobs: &Jobs) {
    let running = jobs
        .list()
        .into_iter()
        .find(|job| job.product == SETUP && job.state == crate::jobs::JobState::Running);
    if let Some(job) = running {
        jobs.wait(&job.id);
    }
}

/// Adds the PECL extensions Werd ships with PHP (phpredis, the MongoDB driver)
/// to PHP lines installed before Werd shipped them.
fn start_extension_backfill(daemon: &Daemon) -> Result<Option<crate::jobs::Job>> {
    let root = daemon.root.clone();
    let catalog = daemon.catalog();
    let installed = Installed::load(&root)?;
    let mut missing: Vec<(String, String)> = Vec::new();
    for line in installed.lines("php") {
        for (extension, library) in runtimes::PHP_PECL_EXTENSIONS {
            let present = installed.version(extension, &line).is_some()
                && runtimes::line_dir(&root, extension, &line)
                    .join(library)
                    .is_file();
            if !present && catalog.build(extension, &line).is_ok() {
                missing.push(((*extension).to_string(), line.clone()));
            }
        }
    }
    if missing.is_empty() {
        return Ok(None);
    }
    let fetcher = Arc::clone(&daemon.fetcher);
    let job = daemon
        .jobs
        .start("php-extensions", "installed", "install", move |progress| {
            for (extension, line) in missing {
                progress.step(&format!("Installing {extension} for PHP {line}"));
                progress.bytes(0, None);
                runtimes::install(&root, &catalog, fetcher.as_ref(), &extension, &line, progress)?;
            }
            Ok(())
        })?;
    Ok(Some(job))
}

/// PHP lines available on this platform, newest first.
fn php_lines(catalog: &Catalog) -> Vec<String> {
    catalog
        .product("php")
        .map(|product| {
            product
                .available_lines()
                .into_iter()
                .map(|(line, _)| line.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Starts a job that creates a Laravel project and links it as a site.
fn create_project(daemon: &Daemon, params: &Value) -> Result<Value> {
    let request: crate::create::NewProject =
        serde_json::from_value(params.clone()).context("Invalid project request")?;
    request.validate(&daemon.root)?;
    let (root, catalog, fetcher, shared) = (
        daemon.root.clone(),
        daemon.catalog(),
        Arc::clone(&daemon.fetcher),
        Arc::clone(&daemon.state),
    );
    let name = request.name.clone();
    let job = daemon.jobs.start("laravel", &name, "create", move |progress| {
        let folder = crate::create::run(&root, &catalog, fetcher.as_ref(), &request, progress)?;
        progress.step("Linking the site");
        let mut state = shared.lock().map_err(|_| anyhow!("Daemon state unavailable"))?;
        let project = projects::add_with(
            &root,
            &mut state,
            &folder.to_string_lossy(),
            None,
            Some(&request.php),
        )?;
        if let Some(url) = projects::site_url(&root, &project) {
            crate::create::set_app_url(&folder, &url)?;
        }
        progress.result(&project.id);
        Ok(())
    })?;
    Ok(json!(job))
}

fn set_default(root: &Path, params: &Value) -> Result<Value> {
    let product = text(params, "product")?;
    let line = text(params, "line")?;
    if Installed::load(root)?.version(product, line).is_none() {
        bail!("Install {product} {line} before making it the default");
    }
    let mut settings = Settings::load(root)?;
    match product {
        "php" => settings.default_php = Some(line.into()),
        "node" => settings.default_node = Some(line.into()),
        other => bail!("{other} has no default version"),
    }
    settings.save(root)?;
    Ok(json!(settings))
}

fn update_settings(root: &Path, state: &mut State, params: &Value) -> Result<Value> {
    let mut settings = Settings::load(root)?;
    let before = (settings.domains, settings.https_port);
    if let Some(value) = params.get("domains") {
        settings.domains = value.as_bool().context("domains must be true or false")?;
    }
    if let Some(value) = params.get("https_port") {
        settings.https_port =
            serde_json::from_value(value.clone()).context("https_port must be a port number")?;
    }
    if let Some(value) = params.get("upload_max_mb") {
        settings.upload_max_mb =
            serde_json::from_value(value.clone()).context("upload_max_mb must be a number")?;
    }
    if let Some(value) = params.get("memory_limit_mb") {
        settings.memory_limit_mb =
            serde_json::from_value(value.clone()).context("memory_limit_mb must be a number")?;
    }
    settings.validate()?;
    settings.save(root)?;
    runtimes::write_all_php_ini(root, &settings)?;
    if before != (settings.domains, settings.https_port) {
        state.restart_router(root)?;
        state.save(root)?;
    }
    Ok(json!(settings))
}

/// Methods that run a slow command in a site folder. They copy the site under
/// the lock and run without it, so other calls are not blocked meanwhile.
fn dispatch_unlocked(
    daemon: &Daemon,
    state: &Mutex<State>,
    method: &str,
    params: &Value,
) -> Option<Result<Value>> {
    let method = method.strip_prefix("sites.").unwrap_or(method);
    if !matches!(method, "about" | "boost") {
        return None;
    }
    let project = (|| {
        let state = state.lock().map_err(|_| anyhow!("Daemon state unavailable"))?;
        Ok::<_, anyhow::Error>(state.project(id(params)?)?.clone())
    })();
    Some(project.and_then(|project| match method {
        "about" => crate::artisan::about(&daemon.root, &project),
        _ => {
            if !crate::artisan::has_boost(&project) {
                bail!("{} does not use Laravel Boost", project.name);
            }
            crate::artisan::boost_update(&daemon.root, &project).map(Value::from)
        }
    }))
}

/// Executes one RPC method. `sites.*` names and the 0.1 names are both accepted.
fn dispatch(daemon: &Daemon, state: &mut State, method: &str, params: &Value) -> Result<Value> {
    let root = daemon.root.as_path();
    let method = method.strip_prefix("sites.").unwrap_or(method);
    Ok(match method {
        "ping" => json!({ "version": VERSION, "protocol": PROTOCOL_VERSION }),
        "list" => json!(Snapshot {
            projects: state.projects.clone(),
            daemon_version: VERSION.into()
        }),
        "add" => {
            let project = projects::add_with(
                root,
                state,
                text(params, "path")?,
                params["name"].as_str(),
                params["php"].as_str(),
            )?;
            // Asked for explicitly by the client (the Add site dialog), like Herd's link.
            if params["update_env"].as_bool() == Some(true) {
                if let Some(url) = projects::site_url(root, &project) {
                    crate::create::set_app_url(Path::new(&project.path), &url)?;
                }
            }
            json!(project)
        }
        "inspect" => {
            let path = crate::parks::display_path(Path::new(text(params, "path")?))?;
            json!(crate::inspect::inspect(
                root,
                Path::new(&path),
                &php_lines(&daemon.catalog())
            ))
        }
        "info" => {
            let path = state.project(id(params)?)?.path.clone();
            json!(crate::inspect::inspect(
                root,
                Path::new(&path),
                &php_lines(&daemon.catalog())
            ))
        }
        "create" => create_project(daemon, params)?,
        "start" => {
            let id = id(params)?;
            // Download Caddy only for a site that could otherwise start; anything
            // else missing (PHP, services) is reported right away.
            let project = state.project(id)?;
            let ready = project.requirements.is_empty()
                && project.status != ProjectStatus::Running
                && runtimes::resolve_line(root, "php", Some(&project.php), "PHP").is_ok();
            if ready && runtimes::resolve_line(root, "caddy", None, "Caddy").is_err() {
                return start_after_caddy(daemon, state, id);
            }
            match projects::start(root, state, id) {
                Ok(project) => json!(project),
                Err(error) => {
                    projects::mark_failed(root, state, id, &error);
                    return Err(error);
                }
            }
        }
        "stop" => json!(projects::stop(root, state, id(params)?)?),
        "remove" => {
            projects::remove(root, state, id(params)?)?;
            json!(null)
        }
        "php" => json!(projects::set_runtime(
            root,
            state,
            id(params)?,
            "php",
            Some(text(params, "line")?)
        )?),
        "node" => json!(projects::set_runtime(
            root,
            state,
            id(params)?,
            "node",
            params["line"].as_str()
        )?),
        "link" => json!(projects::link(
            root,
            state,
            id(params)?,
            text(params, "category")?,
            text(params, "instance")?,
            params["database"].as_str(),
        )?),
        "unlink" => json!(projects::unlink(
            root,
            state,
            id(params)?,
            text(params, "category")?
        )?),
        "resolve" => resolve_site(daemon, state, id(params)?)?,
        "domain" => json!(projects::set_domain(
            root,
            state,
            id(params)?,
            text(params, "domain")?
        )?),
        "reset-ports" => json!(projects::reset_ports(root, state, id(params)?)?),
        "open" => json!(projects::open(state, id(params)?)?),
        "logs" => {
            let id = id(params)?;
            state.project(id)?;
            json!(process::read_log(
                root,
                id,
                params["service"].as_str().unwrap_or("werd")
            )?)
        }
        "env" => json!(projects::env(root, state, id(params)?)?),
        "doctor" => json!(doctor::run(
            root,
            &daemon.catalog(),
            &state.router,
            &projects::domains(state)
        )),
        "trust-ca" => json!(projects::trust_local_ca(root)?),
        "certificate.status" => projects::local_ca_status(root),

        "system.info" => {
            let catalog = daemon.catalog();
            let settings = Settings::load(root)?;
            json!({
                "version": VERSION,
                "protocol": PROTOCOL_VERSION,
                "home": root,
                "bin": shims::bin_dir(root),
                "platform": crate::catalog::PLATFORM,
                "catalog_generated": catalog.generated,
                "catalog_refreshable": settings.catalog_url.is_some(),
                "path_enabled": settings.path_enabled,
            })
        }
        "domains.status" => {
            let settings = Settings::load(root)?;
            let domains = projects::domains(state);
            json!({
                "enabled": settings.domains,
                "https_port": settings.https_port,
                "active": state.router.domains_active(),
                "warning": state.router.warning,
                "domains": domains,
                "missing": if settings.domains { crate::domains::missing_from_hosts(&domains) } else { Vec::new() },
            })
        }
        "parks.list" => json!(Settings::load(root)?.parked),
        "parks.add" => json!(crate::parks::add(root, state, text(params, "path")?)?),
        "parks.remove" => json!(crate::parks::remove(root, state, text(params, "path")?)?),
        "path.enable" => {
            shims::enable(root)?;
            json!(Settings::load(root)?)
        }
        "path.disable" => {
            shims::disable(root)?;
            json!(Settings::load(root)?)
        }
        "catalog.get" => {
            let catalog = daemon.catalog();
            json!({ "generated": catalog.generated, "platform": crate::catalog::PLATFORM,
                    "refreshable": Settings::load(root)?.catalog_url.is_some() })
        }
        "catalog.refresh" => {
            let url = Settings::load(root)?
                .catalog_url
                .context("Online catalog updates are not available yet; update Werd to get newer runtimes")?;
            let catalog = Catalog::refresh(root, &url)?;
            json!({ "generated": catalog.generated })
        }
        "runtimes" | "runtimes.list" => {
            json!(runtimes::list(root, &daemon.catalog(), &Settings::load(root)?)?)
        }
        "runtimes.install" => start_install(daemon, state, params, false)?,
        "runtimes.update" => start_install(daemon, state, params, true)?,
        "runtimes.uninstall" => {
            let (product, line) = (text(params, "product")?, text(params, "line")?);
            let users = runtime_users(state, product, line);
            if !users.is_empty() {
                bail!("Stop {} before removing {product} {line}", users.join(", "));
            }
            runtimes::uninstall(root, product, line)?;
            json!(null)
        }
        "runtimes.default" => set_default(root, params)?,

        "services.catalog" => json!(instances::offerings(root, &daemon.catalog())?),
        "services.list" => json!(instances::summaries(root, &state.instances)),
        "services.create" => create_service(daemon, state, params)?,
        "services.start" => json!(instances::start(root, &mut state.instances, id(params)?)?),
        "services.stop" => json!(instances::stop(root, &mut state.instances, id(params)?)?),
        "services.delete" => {
            let keep_data = params["keep_data"].as_bool().unwrap_or(false);
            instances::delete(root, &mut state.instances, id(params)?, keep_data)?;
            json!(null)
        }
        "services.autostart" => {
            let enabled = params["autostart"].as_bool().context("Missing autostart")?;
            json!(instances::set_autostart(
                root,
                &mut state.instances,
                id(params)?,
                enabled
            )?)
        }
        "services.rename" => json!(instances::rename(
            root,
            &mut state.instances,
            id(params)?,
            text(params, "name")?
        )?),
        "services.details" => instances::details(root, &state.instances, id(params)?)?,
        "services.logs" => json!(instances::logs(root, &state.instances, id(params)?)?),
        "services.database" => {
            json!(instances::create_database(
                root,
                &state.instances,
                id(params)?,
                text(params, "name")?
            )?)
        }
        "jobs.list" => json!(daemon.jobs.list()),
        "setup.run" => json!(start_setup(daemon, true)?),
        "settings.get" => json!(Settings::load(root)?),
        "settings.set" => update_settings(root, state, params)?,
        other => bail!("Unknown method: {other}"),
    })
}

/// Marks projects whose processes died as failed and stops their other processes.
fn reap_exited(root: &Path, state: &mut State) {
    let mut failed = Vec::new();
    if let Some(exit) = state.router.has_exited() {
        for id in state.processes.keys() {
            failed.push((id.clone(), "caddy".to_string(), exit.clone()));
        }
    }
    for (id, children) in &mut state.processes {
        if let Some((name, exit)) = children
            .iter_mut()
            .find_map(|child| child.has_exited().map(|exit| (child.name.clone(), exit)))
        {
            failed.push((id.clone(), name, exit));
        }
    }
    instances::reap_exited(root, &mut state.instances);
    let restart_router = !failed.is_empty();
    for (id, name, exit) in failed {
        projects::stop_processes(state, &id);
        if let Ok(index) = state.index(&id) {
            let project = &mut state.projects[index];
            project.status = ProjectStatus::Error;
            project.error = Some(format!("Process {name} exited: {exit}"));
            project.url = None;
        }
        let _ = append_log(root, &id, &format!("Error: {name} exited: {exit}"));
        let _ = state.save(root);
    }
    if restart_router {
        let _ = state.sync_router(root);
    }
}

pub fn run_daemon() -> Result<()> {
    let root: PathBuf = home()?;
    if rpc::rpc("ping", json!({})).is_ok() {
        bail!("A Werd daemon is already running");
    }
    if let Err(error) = process::kill_children_on_exit() {
        eprintln!("Werd daemon: {error:#}; processes may outlive a crash");
    }
    migrations::run(&root).context("Upgrading the Werd data folder failed")?;
    shims::refresh(&root);
    let state = Arc::new(Mutex::new(State::load(&root)?));
    let daemon = Daemon {
        root: root.clone(),
        jobs: Jobs::default(),
        fetcher: Arc::new(HttpFetcher),
        catalog: None,
        state: Arc::clone(&state),
    };
    let (listener, endpoint) = rpc::listen(&root)?;

    // Prepare a fresh install (Caddy, PHP, Composer) without blocking startup.
    let _ = start_setup(&daemon, false);

    // New Werd versions may enable more bundled extensions: rewrite php.ini of
    // every installed line so they apply without reinstalling PHP.
    if let Ok(settings) = Settings::load(&root) {
        let _ = runtimes::write_all_php_ini(&root, &settings);
    }

    // Setup installs PECL extensions with new PHP lines. Backfill older
    // installations after it finishes, without delaying the API or services.
    let backfill = daemon.clone();
    thread::spawn(move || {
        wait_for_setup(&backfill.jobs);
        if let Err(error) = start_extension_backfill(&backfill) {
            eprintln!("Werd PHP extensions: {error:#}");
        }
    });

    // Start autostart services after the API is up, so clients never wait for them.
    let autostart = Arc::clone(&state);
    let autostart_root = root.clone();
    thread::spawn(move || {
        let ids: Vec<String> = autostart
            .lock()
            .map(|state| {
                state
                    .instances
                    .list
                    .iter()
                    .filter(|i| i.autostart)
                    .map(|i| i.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            if let Ok(mut state) = autostart.lock() {
                let _ = instances::start(&autostart_root, &mut state.instances, &id);
            }
        }
    });

    if let Ok(mut state) = state.lock() {
        let _ = crate::parks::scan(&root, &mut state);
    }
    let monitor = Arc::clone(&state);
    thread::spawn(move || {
        let mut tick = 0u64;
        loop {
            thread::sleep(MONITOR_INTERVAL);
            tick = tick.wrapping_add(1);
            if let Ok(mut state) = monitor.lock() {
                reap_exited(&root, &mut state);
                if tick.is_multiple_of(PARKS_SCAN_TICKS) {
                    let _ = crate::parks::scan(&root, &mut state);
                }
            }
        }
    });

    for stream in listener.incoming().flatten() {
        let daemon = daemon.clone();
        let state = Arc::clone(&state);
        let token = endpoint.token.clone();
        thread::spawn(move || {
            let _ = rpc::serve_connection(stream, &token, |method, params| {
                if let Some(result) = dispatch_unlocked(&daemon, &state, method, &params) {
                    return result;
                }
                let mut state = state.lock().map_err(|_| anyhow!("Daemon state unavailable"))?;
                dispatch(&daemon, &mut state, method, &params)
            });
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::JobState;
    use crate::runtimes::tests::{fixture, php_extensions_fixture, LocalFetcher};

    fn daemon(root: &Path, catalog: Option<(Catalog, LocalFetcher)>) -> Daemon {
        let (catalog, fetcher): (Option<Arc<Catalog>>, Arc<dyn Fetcher>) = match catalog {
            Some((catalog, fetcher)) => (Some(Arc::new(catalog)), Arc::new(fetcher)),
            None => (None, Arc::new(HttpFetcher)),
        };
        Daemon {
            root: root.to_path_buf(),
            jobs: Jobs::default(),
            fetcher,
            catalog,
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    #[test]
    fn dispatch_answers_ping_list_and_rejects_unknown_methods() {
        let root = tempfile::tempdir().unwrap();
        let daemon = daemon(root.path(), None);
        let mut state = State::default();
        let ping = dispatch(&daemon, &mut state, "ping", &json!({})).unwrap();
        assert_eq!(ping["protocol"], PROTOCOL_VERSION);
        let list = dispatch(&daemon, &mut state, "sites.list", &json!({})).unwrap();
        assert_eq!(list["projects"], json!([]));
        assert!(dispatch(&daemon, &mut state, "rm -rf", &json!({})).is_err());
        assert!(dispatch(&daemon, &mut state, "stop", &json!({}))
            .unwrap_err()
            .to_string()
            .contains("id"));
    }

    #[test]
    fn failed_start_is_recorded_on_the_project() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        std::fs::write(work.path().join("artisan"), "").unwrap();
        std::fs::write(work.path().join("composer.json"), "{}").unwrap();
        let daemon = daemon(root.path(), None);
        let mut state = State::default();
        let added = dispatch(&daemon, &mut state, "add", &json!({ "path": work.path() })).unwrap();
        let id = added["id"].as_str().unwrap().to_string();

        assert!(dispatch(&daemon, &mut state, "start", &json!({ "id": id })).is_err());
        let project = state.project(&id).unwrap();
        assert_eq!(project.status, ProjectStatus::Error);
        assert!(project.error.as_deref().unwrap().contains("not installed"));
    }

    #[test]
    fn runtimes_install_in_the_background_then_update_and_uninstall() {
        let root = tempfile::tempdir().unwrap();
        let daemon = daemon(root.path(), Some(fixture(root.path(), &[("1", "1.0.0")])));
        let mut state = State::default();
        let params = json!({ "product": "tool", "line": "1" });

        assert!(dispatch(&daemon, &mut state, "runtimes.update", &params)
            .unwrap_err()
            .to_string()
            .contains("not installed"));
        let job = dispatch(&daemon, &mut state, "runtimes.install", &params).unwrap();
        let finished = daemon.jobs.wait(job["id"].as_str().unwrap());
        assert_eq!(finished.state, JobState::Done, "{:?}", finished.error);

        let rows = dispatch(&daemon, &mut state, "runtimes.list", &json!({})).unwrap();
        assert_eq!(rows[0]["installed"], "1.0.0");
        assert!(dispatch(&daemon, &mut state, "runtimes.default", &params)
            .unwrap_err()
            .to_string()
            .contains("no default"));

        dispatch(&daemon, &mut state, "runtimes.uninstall", &params).unwrap();
        let rows = dispatch(&daemon, &mut state, "runtimes.list", &json!({})).unwrap();
        assert!(rows[0]["installed"].is_null());
    }

    #[test]
    fn startup_backfills_pecl_extensions_for_existing_php() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, fetcher) = php_extensions_fixture(root.path());
        let php = runtimes::line_dir(root.path(), "php", "8.5");
        std::fs::create_dir_all(&php).unwrap();
        std::fs::write(php.join("php-cgi.exe"), "php").unwrap();
        let mut installed = Installed::default();
        installed.set("php", "8.5", "8.5.11");
        installed.save(root.path()).unwrap();
        let daemon = daemon(root.path(), Some((catalog, fetcher)));
        let job = start_extension_backfill(&daemon).unwrap().unwrap();
        let finished = daemon.jobs.wait(&job.id);
        assert_eq!(finished.state, JobState::Done, "{:?}", finished.error);
        assert!(runtimes::line_dir(root.path(), "phpredis", "8.5")
            .join("php_redis.dll")
            .is_file());
        if cfg!(windows) {
            assert!(std::fs::read_to_string(php.join("php.ini"))
                .unwrap()
                .contains("php_redis.dll"));
        }
        assert!(runtimes::line_dir(root.path(), "phpmongodb", "8.5")
            .join("php_mongodb.dll")
            .is_file());
        assert!(start_extension_backfill(&daemon).unwrap().is_none());
    }

    #[test]
    fn settings_are_validated_and_saved() {
        let root = tempfile::tempdir().unwrap();
        let daemon = daemon(root.path(), None);
        let mut state = State::default();
        let saved = dispatch(
            &daemon,
            &mut state,
            "settings.set",
            &json!({ "upload_max_mb": 256 }),
        )
        .unwrap();
        assert_eq!(saved["upload_max_mb"], 256);
        assert!(dispatch(
            &daemon,
            &mut state,
            "settings.set",
            &json!({ "memory_limit_mb": 1 })
        )
        .is_err());
        assert_eq!(
            dispatch(&daemon, &mut state, "settings.get", &json!({})).unwrap()["upload_max_mb"],
            256
        );
        assert!(dispatch(&daemon, &mut state, "catalog.refresh", &json!({}))
            .unwrap_err()
            .to_string()
            .contains("not available"));
    }
}
