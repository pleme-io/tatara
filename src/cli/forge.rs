use anyhow::{bail, Context, Result};

use super::output::{render_value, OutputFormat};

/// Scaffold a new forge from template.
pub async fn init(name: &str) -> Result<()> {
    let dir = std::path::PathBuf::from(name);
    if dir.exists() {
        bail!("Directory '{}' already exists", name);
    }

    tokio::fs::create_dir_all(&dir).await?;

    // Generate flake.nix using the Nix forge template
    let output = tokio::process::Command::new("nix")
        .args([
            "eval",
            "--raw",
            "--impure",
            "--expr",
            &format!(
                "let forge = import {}/nix/lib/forge.nix {{}}; in forge.forgeTemplate \"{}\"",
                env!("CARGO_MANIFEST_DIR"),
                name
            ),
        ])
        .output()
        .await;

    let flake_content = match output {
        Ok(out) if out.status.success() => String::from_utf8(out.stdout)?,
        _ => {
            // Fallback: generate template inline
            generate_template(name)
        }
    };

    let flake_path = dir.join("flake.nix");
    tokio::fs::write(&flake_path, &flake_content).await?;

    println!("Forge '{}' created at {}/", name, name);
    println!();
    println!("Next steps:");
    println!("  cd {}", name);
    println!("  # Edit flake.nix to configure your workload");
    println!("  tatara forge validate .");
    println!("  tatara deploy .");

    Ok(())
}

/// Validate a forge at the given path.
pub async fn validate(path: &str, output: OutputFormat) -> Result<()> {
    let flake_path = std::path::PathBuf::from(path).join("flake.nix");
    if !flake_path.exists() {
        bail!("No flake.nix found at {}", path);
    }

    println!("Validating forge at {}...", path);

    // Check that `nix eval` can evaluate the flake outputs
    let mut errors: Vec<String> = Vec::new();

    // Check tataraMeta
    let meta_result = tokio::process::Command::new("nix")
        .args(["eval", "--json", &format!("{}#tataraMeta", path)])
        .output()
        .await;

    match meta_result {
        Ok(out) if out.status.success() => {
            let meta: serde_json::Value = serde_json::from_slice(&out.stdout)
                .unwrap_or(serde_json::Value::Null);
            if meta.get("name").is_none() {
                errors.push("tataraMeta missing 'name' field".to_string());
            }
            if meta.get("version").is_none() {
                errors.push("tataraMeta missing 'version' field".to_string());
            }
            println!("  tataraMeta: OK");
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            errors.push(format!("tataraMeta eval failed: {}", stderr.lines().next().unwrap_or("")));
        }
        Err(e) => {
            errors.push(format!("Failed to run nix eval: {}", e));
        }
    }

    // Check tataraJobs
    let system = std::env::consts::ARCH.replace("aarch64", "aarch64").replace("x86_64", "x86_64");
    let os = std::env::consts::OS.replace("macos", "darwin").replace("linux", "linux");
    let nix_system = format!("{}-{}", system, os);

    let jobs_result = tokio::process::Command::new("nix")
        .args([
            "eval",
            "--json",
            &format!("{}#tataraJobs.{}", path, nix_system),
        ])
        .output()
        .await;

    match jobs_result {
        Ok(out) if out.status.success() => {
            let jobs: serde_json::Value = serde_json::from_slice(&out.stdout)
                .unwrap_or(serde_json::Value::Null);
            if let Some(obj) = jobs.as_object() {
                println!("  tataraJobs.{}: OK ({} jobs)", nix_system, obj.len());
                for (name, _spec) in obj {
                    println!("    - {}", name);
                }
            } else {
                errors.push("tataraJobs is not an object".to_string());
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            errors.push(format!(
                "tataraJobs.{} eval failed: {}",
                nix_system,
                stderr.lines().next().unwrap_or("")
            ));
        }
        Err(e) => {
            errors.push(format!("Failed to run nix eval: {}", e));
        }
    }

    if errors.is_empty() {
        println!("\nForge is valid!");
    } else {
        println!("\nValidation errors:");
        for err in &errors {
            println!("  - {}", err);
        }
        bail!("Forge validation failed with {} errors", errors.len());
    }

    Ok(())
}

/// Inspect a forge's metadata and job specs without deploying.
pub async fn inspect(flake_ref: &str, output: OutputFormat) -> Result<()> {
    // Evaluate tataraMeta
    let meta_output = tokio::process::Command::new("nix")
        .args(["eval", "--json", &format!("{}#tataraMeta", flake_ref)])
        .output()
        .await
        .context("Failed to run nix eval")?;

    let meta: serde_json::Value = if meta_output.status.success() {
        serde_json::from_slice(&meta_output.stdout)?
    } else {
        serde_json::json!({ "error": "Could not evaluate tataraMeta" })
    };

    // Evaluate tataraJobs for current system
    let system = std::env::consts::ARCH;
    let os = std::env::consts::OS.replace("macos", "darwin");
    let nix_system = format!("{}-{}", system, os);

    let jobs_output = tokio::process::Command::new("nix")
        .args([
            "eval",
            "--json",
            &format!("{}#tataraJobs.{}", flake_ref, nix_system),
        ])
        .output()
        .await
        .context("Failed to run nix eval")?;

    let jobs: serde_json::Value = if jobs_output.status.success() {
        serde_json::from_slice(&jobs_output.stdout)?
    } else {
        serde_json::json!({ "error": "Could not evaluate tataraJobs" })
    };

    let result = serde_json::json!({
        "meta": meta,
        "jobs": jobs,
        "system": nix_system,
    });

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&result, output)?);
        }
        _ => {
            println!("Forge: {}", meta.get("name").and_then(|n| n.as_str()).unwrap_or("?"));
            println!("Version: {}", meta.get("version").and_then(|v| v.as_str()).unwrap_or("?"));
            if let Some(desc) = meta.get("description").and_then(|d| d.as_str()) {
                println!("Description: {}", desc);
            }
            println!("System: {}", nix_system);
            println!();

            if let Some(obj) = jobs.as_object() {
                println!("Jobs:");
                for (name, spec) in obj {
                    println!("  {}:", name);
                    println!("    Type:   {}", spec["job_type"].as_str().unwrap_or("?"));
                    if let Some(groups) = spec["groups"].as_array() {
                        for group in groups {
                            let task_count = group["tasks"].as_array().map(|t| t.len()).unwrap_or(0);
                            println!(
                                "    Group '{}': {} tasks, {} replicas",
                                group["name"].as_str().unwrap_or("?"),
                                task_count,
                                group["count"].as_u64().unwrap_or(1),
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn generate_template(name: &str) -> String {
    format!(
        r#"{{
  description = "{name} — tatara forge";

  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    tatara.url = "github:pleme-io/tatara";
  }};

  outputs = {{ self, nixpkgs, tatara, ... }}:
  let
    systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
    forEachSystem = nixpkgs.lib.genAttrs systems;
  in
  {{
    tataraJobs = forEachSystem (system: {{
      {name} = {{
        id = "{name}";
        job_type = "service";
        groups = [
          {{
            name = "main";
            count = 1;
            tasks = [
              {{
                name = "app";
                driver = "nix";
                config = {{
                  type = "nix";
                  flake_ref = "github:you/{name}";
                }};
                env = {{}};
                resources = {{ cpu_mhz = 500; memory_mb = 256; }};
                health_checks = [];
              }}
            ];
            restart_policy = {{ mode = "on_failure"; attempts = 3; interval_secs = 300; delay_secs = 5; }};
            resources = {{ cpu_mhz = 0; memory_mb = 0; }};
          }}
        ];
        constraints = [];
        meta = {{}};
      }};
    }});

    tataraMeta = {{
      name = "{name}-forge";
      version = "1.0.0";
      description = "{name} workload for tatara";
    }};
  }};
}}"#
    )
}
