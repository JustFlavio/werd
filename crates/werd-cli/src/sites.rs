//! Site configuration commands: info, set, link, resolve.

use crate::call;
use crate::runtimes::wait;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use werd_core::jobs::Job;

fn service_name(instances: &[Value], id: &str) -> String {
    instances
        .iter()
        .find(|instance| instance["id"] == id)
        .and_then(|instance| instance["name"].as_str())
        .unwrap_or(id)
        .to_string()
}

pub fn info(id: &str, json_output: bool) -> Result<()> {
    let snapshot = call("sites.list", json!({}))?;
    let site = snapshot["projects"]
        .as_array()
        .and_then(|projects| projects.iter().find(|project| project["id"] == id))
        .cloned()
        .unwrap_or_default();
    if json_output {
        println!("{}", serde_json::to_string_pretty(&site)?);
        return Ok(());
    }
    let instances: Vec<Value> = serde_json::from_value(call("services.list", json!({}))?)?;
    println!(
        "{}  {}",
        site["name"].as_str().unwrap_or_default(),
        site["path"].as_str().unwrap_or_default()
    );
    println!("status   {}", site["status"].as_str().unwrap_or_default());
    println!("php      {}", site["php"].as_str().unwrap_or("-"));
    println!("node     {}", site["node"].as_str().unwrap_or("default"));
    if let Some(url) = site["url"].as_str() {
        println!("url      {url}");
    }
    if let Some(links) = site["links"].as_object().filter(|links| !links.is_empty()) {
        println!("\nservices");
        for (category, link) in links {
            let database = link["database"]
                .as_str()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default();
            println!(
                "  {category:9} {}{database}",
                service_name(&instances, link["instance"].as_str().unwrap_or_default())
            );
        }
    }
    if let Some(pending) = site["requirements"]
        .as_array()
        .filter(|pending| !pending.is_empty())
    {
        println!(
            "\nstill needed (run `werd resolve {}`)",
            site["name"].as_str().unwrap_or_default()
        );
        for requirement in pending {
            let line = requirement["line"]
                .as_str()
                .map(|line| format!(" {line}"))
                .unwrap_or_default();
            println!(
                "  {:9} {}{line}",
                requirement["category"].as_str().unwrap_or_default(),
                requirement["product"].as_str().unwrap_or_default()
            );
        }
    }
    Ok(())
}

pub fn set(id: &str, php: Option<&str>, node: Option<&str>) -> Result<()> {
    if php.is_none() && node.is_none() {
        bail!("Pass --php <version> and/or --node <version>");
    }
    if let Some(line) = php {
        call("sites.php", json!({ "id": id, "line": line }))?;
        println!("PHP {line} will be used from the next start");
    }
    if let Some(line) = node {
        call("sites.node", json!({ "id": id, "line": line }))?;
        println!("Node.js {line} is used inside this site");
    }
    Ok(())
}

pub fn link(id: &str, category: &str, service: &str, database: Option<&str>) -> Result<()> {
    let instances: Vec<Value> = serde_json::from_value(call("services.list", json!({}))?)?;
    let instance = instances
        .iter()
        .find(|instance| {
            instance["id"] == service
                || instance["name"]
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(service))
        })
        .and_then(|instance| instance["id"].as_str())
        .map(str::to_string);
    let Some(instance) = instance else {
        bail!("No service named {service}. Run `werd service` to list them.")
    };
    call(
        "sites.link",
        json!({ "id": id, "category": category, "instance": instance, "database": database }),
    )?;
    println!("Linked {category} to {service}. Run `werd env` for the new .env values.");
    Ok(())
}

pub fn resolve_services(id: &str) -> Result<()> {
    let result = call("sites.resolve", json!({ "id": id }))?;
    for job in result["jobs"].as_array().into_iter().flatten() {
        wait(serde_json::from_value::<Job>(job.clone())?)?;
    }
    let links = result["project"]["links"]
        .as_object()
        .map_or(0, serde_json::Map::len);
    println!("{links} service(s) linked. Start the site with `werd up`.");
    Ok(())
}
