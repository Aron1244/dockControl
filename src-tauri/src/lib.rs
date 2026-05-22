use std::{path::Path, process::Command, thread, time::Duration};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use serde::Serialize;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Serialize)]
struct ActionProgress {
    step: String,
    message: String,
}

#[derive(Serialize)]
struct CheckResult {
    ready: bool,
    message: String,
}

#[derive(Serialize)]
struct ProcessSnapshot {
    processes: Vec<String>,
}

#[derive(Serialize)]
struct StartupProbe {
    ready: bool,
    message: String,
    processes: Vec<String>,
}

fn compose_up_with_retries(compose_dir: &str, attempts: u32, delay_seconds: u64) -> Result<(), String> {
    for attempt in 1..=attempts {
        let mut up_command = Command::new("docker");
        up_command.args(["compose", "up", "-d"]);
        up_command.current_dir(compose_dir);

        match run_hidden_output(up_command) {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) if attempt < attempts => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let _ = (stderr, stdout);
                thread::sleep(Duration::from_secs(delay_seconds));
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let details = match (stderr.is_empty(), stdout.is_empty()) {
                    (false, false) => format!("{stderr} | {stdout}"),
                    (false, true) => stderr,
                    (true, false) => stdout,
                    (true, true) => "sin detalles".to_string(),
                };

                return Err(format!("docker compose up -d no pudo iniciarse: {details}"));
            }
            Err(error) if attempt < attempts => {
                let _ = error;
                thread::sleep(Duration::from_secs(delay_seconds));
            }
            Err(error) => return Err(format!("docker compose up -d no pudo iniciarse: {error}")),
        }
    }

    Err("docker compose up -d no pudo iniciarse".to_string())
}

fn wait_for_docker_ready(timeout_seconds: u64) -> Result<(), String> {
    for _ in 0..timeout_seconds {
        let mut command = Command::new("docker");
        command.args(["info"]);

        if matches!(run_hidden_status(command), Ok(status) if status.success()) {
            return Ok(());
        }

        thread::sleep(Duration::from_secs(1));
    }

    Err("Docker Engine no quedó listo a tiempo".to_string())
}

fn run_hidden_status(mut command: Command) -> Result<std::process::ExitStatus, String> {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
        .status()
        .map_err(|error| format!("No se pudo ejecutar el comando: {error}"))
}

fn run_hidden_output(mut command: Command) -> Result<std::process::Output, String> {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
        .output()
        .map_err(|error| format!("No se pudo ejecutar el comando: {error}"))
}

fn resolve_docker_cli_path(docker_desktop_path: &str) -> Option<std::path::PathBuf> {
    let mut current = Path::new(docker_desktop_path).parent();

    for _ in 0..3 {
        if let Some(dir) = current {
            let candidate = dir.join("DockerCli.exe");
            if candidate.exists() {
                return Some(candidate);
            }

            current = dir.parent();
        } else {
            break;
        }
    }

    None
}

fn run_hidden_spawn(mut command: Command) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("No se pudo ejecutar el comando: {error}"))
}

fn capture_docker_process_snapshot() -> Result<Vec<String>, String> {
    let mut command = Command::new("tasklist");
    command.args([
        "/FI",
        "IMAGENAME eq Docker Desktop.exe",
        "/FI",
        "IMAGENAME eq Docker Desktop Backend.exe",
        "/FI",
        "IMAGENAME eq Docker Desktop Launcher.exe",
        "/FI",
        "IMAGENAME eq docker-sandbox.exe",
        "/FI",
        "IMAGENAME eq com.docker.backend.exe",
        "/FI",
        "IMAGENAME eq com.docker.desktop.exe",
        "/FI",
        "IMAGENAME eq vpnkit.exe",
        "/FI",
        "IMAGENAME eq docker.exe",
    ]);

    let output = run_hidden_output(command)?;
    let text = String::from_utf8_lossy(&output.stdout);

    let mut processes = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Docker Desktop")
            || trimmed.starts_with("Docker Desktop Backend")
            || trimmed.starts_with("Docker Desktop Launcher")
            || trimmed.starts_with("docker-sandbox")
            || trimmed.starts_with("com.docker.backend")
            || trimmed.starts_with("com.docker.desktop")
            || trimmed.starts_with("vpnkit")
            || trimmed.starts_with("docker")
        {
            processes.push(trimmed.to_string());
        }
    }

    Ok(processes)
}

fn snapshot_contains(snapshot: &[String], needle: &str) -> bool {
    snapshot.iter().any(|process| process.to_lowercase().contains(needle))
}

fn wait_for_processes_gone(process_names: &[&str], timeout_seconds: u64) -> Result<(), String> {
    for _ in 0..timeout_seconds {
        let processes = capture_docker_process_snapshot()?;
        if !process_names.iter().any(|name| snapshot_contains(&processes, name)) {
            return Ok(());
        }

        thread::sleep(Duration::from_secs(1));
    }

    let remaining = capture_docker_process_snapshot()?;
    if process_names.iter().any(|name| snapshot_contains(&remaining, name)) {
        let filtered: Vec<String> = remaining
            .into_iter()
            .filter(|process| process_names.iter().any(|name| process.to_lowercase().contains(name)))
            .collect();

        if filtered.is_empty() {
            Ok(())
        } else {
            Err(format!("Procesos Docker aún activos: {}", filtered.join(" | ")))
        }
    } else {
        Ok(())
    }
}

#[tauri::command]
async fn start_docker_desktop(docker_desktop_path: String) -> Result<ActionProgress, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !Path::new(&docker_desktop_path).exists() {
            return Err("La ruta de Docker Desktop no existe".to_string());
        }

        let mut launch_command = Command::new("cmd");
        launch_command.args(["/C", "start", "/MIN", "", &docker_desktop_path]);
        run_hidden_spawn(launch_command)?;

        Ok(ActionProgress {
            step: "docker-started".to_string(),
            message: "Docker Desktop se abrió".to_string(),
        })
    })
    .await
    .map_err(|error| format!("No se pudo abrir Docker Desktop: {error}"))?
}

#[tauri::command]
async fn wait_docker_ready() -> Result<ActionProgress, String> {
    tauri::async_runtime::spawn_blocking(move || {
        wait_for_docker_ready(180)?;

        Ok(ActionProgress {
            step: "docker-ready".to_string(),
            message: "Docker Engine ya está listo".to_string(),
        })
    })
    .await
    .map_err(|error| format!("No se pudo confirmar Docker listo: {error}"))?
}

#[tauri::command]
async fn check_docker_ready() -> Result<CheckResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = Command::new("docker");
        command.args(["info"]);

        let ready = matches!(run_hidden_status(command), Ok(status) if status.success());

        Ok(CheckResult {
            ready,
            message: if ready {
                "Docker Engine ya está listo".to_string()
            } else {
                "Docker Engine todavía no responde".to_string()
            },
        })
    })
    .await
    .map_err(|error| format!("No se pudo comprobar Docker: {error}"))?
}

#[tauri::command]
async fn probe_docker_startup() -> Result<StartupProbe, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = Command::new("docker");
        command.args(["info"]);

        let ready = matches!(run_hidden_status(command), Ok(status) if status.success());
        let processes = capture_docker_process_snapshot()?;

        Ok(StartupProbe {
            ready,
            message: if ready {
                "Docker Engine ya está listo".to_string()
            } else if processes.is_empty() {
                "Docker Desktop todavía no muestra procesos".to_string()
            } else {
                format!("Docker iniciando: {}", processes.join(" | "))
            },
            processes,
        })
    })
    .await
    .map_err(|error| format!("No se pudo evaluar el arranque de Docker: {error}"))?
}

#[tauri::command]
async fn start_compose(compose_dir: String) -> Result<ActionProgress, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !Path::new(&compose_dir).exists() {
            return Err("La ruta del proyecto no existe".to_string());
        }

        compose_up_with_retries(&compose_dir, 10, 3)?;

        Ok(ActionProgress {
            step: "compose-started".to_string(),
            message: "El contenedor se levantó".to_string(),
        })
    })
    .await
    .map_err(|error| format!("No se pudo levantar el compose: {error}"))?
}

#[tauri::command]
async fn stop_compose(compose_dir: String) -> Result<ActionProgress, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !Path::new(&compose_dir).exists() {
            return Err("La ruta del proyecto no existe".to_string());
        }

        let mut last_error = String::new();
        for attempt in 1..=5 {
            let mut down_command = Command::new("docker");
            down_command.args(["compose", "down", "--timeout", "15", "--remove-orphans"]);
            down_command.current_dir(&compose_dir);
            let down_output = run_hidden_output(down_command)?;

            if down_output.status.success() {
                return Ok(ActionProgress {
                    step: "compose-stopped".to_string(),
                    message: "El contenedor se bajó".to_string(),
                });
            }

            let stderr = String::from_utf8_lossy(&down_output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&down_output.stdout).trim().to_string();
            last_error = match (stderr.is_empty(), stdout.is_empty()) {
                (false, false) => format!("{stderr} | {stdout}"),
                (false, true) => stderr,
                (true, false) => stdout,
                (true, true) => "sin detalles".to_string(),
            };

            if !last_error.to_lowercase().contains("failed to connect to the docker api") && attempt < 5 {
                thread::sleep(Duration::from_secs(3));
                continue;
            }

            if attempt == 5 {
                return Err(format!("docker compose down falló: {last_error}"));
            }
        }

        Err(format!("docker compose down falló: {last_error}"))
    })
    .await
    .map_err(|error| format!("No se pudo bajar el compose: {error}"))?
}

fn stop_docker_service() -> Result<(), String> {
    let mut sc_command = Command::new("sc");
    sc_command.args(["stop", "com.docker.service"]);
    let _ = run_hidden_output(sc_command);

    for _ in 0..15 {
        let mut query = Command::new("sc");
        query.args(["query", "com.docker.service"]);
        let output = run_hidden_output(query)?;
        let text = String::from_utf8_lossy(&output.stdout);
        if text.contains("STOPPED") {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }

    Err("com.docker.service no se detuvo a tiempo".to_string())
}

#[tauri::command]
async fn shutdown_docker_desktop(docker_desktop_path: String) -> Result<ActionProgress, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(docker_cli) = resolve_docker_cli_path(&docker_desktop_path) {
            let mut shutdown_command = Command::new(docker_cli);
            shutdown_command.args(["-Shutdown"]);
            let _ = run_hidden_spawn(shutdown_command);
        }

        let mut wsl_shutdown = Command::new("wsl");
        wsl_shutdown.args(["--shutdown"]);
        let _ = run_hidden_spawn(wsl_shutdown);

        let _ = stop_docker_service();

        Ok(ActionProgress {
            step: "docker-shutdown-requested".to_string(),
            message: "Se pidió cerrar Docker Desktop".to_string(),
        })
    })
    .await
    .map_err(|error| format!("No se pudo pedir el cierre de Docker: {error}"))?
}

#[tauri::command]
async fn wait_docker_stopped() -> Result<ActionProgress, String> {
    tauri::async_runtime::spawn_blocking(move || {
        wait_for_processes_gone(&["docker desktop backend", "docker desktop.exe"], 20)?;
        wait_for_processes_gone(&["docker desktop launcher", "docker desktop.exe"], 20)?;
        wait_for_processes_gone(&["docker-sandbox.exe"], 20)?;
        wait_for_processes_gone(&["vpnkit.exe"], 15)?;

        let processes = capture_docker_process_snapshot()?;

        Ok(ActionProgress {
            step: "docker-stopped".to_string(),
            message: if processes.is_empty() {
                "Docker Desktop se cerró".to_string()
            } else {
                format!("Docker todavía muestra procesos activos: {}", processes.join(" | "))
            },
        })
    })
    .await
    .map_err(|error| format!("No se pudo confirmar el cierre de Docker: {error}"))?
}

#[tauri::command]
async fn snapshot_docker_processes() -> Result<ProcessSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let processes = capture_docker_process_snapshot()?;
        Ok(ProcessSnapshot { processes })
    })
    .await
    .map_err(|error| format!("No se pudo leer el estado de procesos: {error}"))?
}

fn cleanup_leftover_docker() -> Result<(), String> {
    let _ = stop_docker_service();

    let mut wsl_shutdown = Command::new("wsl");
    wsl_shutdown.args(["--shutdown"]);
    let _ = run_hidden_spawn(wsl_shutdown);

    for process in &[
        "Docker Desktop.exe",
        "Docker Desktop Backend.exe",
        "Docker Desktop Launcher.exe",
        "docker-sandbox.exe",
        "com.docker.backend.exe",
        "com.docker.desktop.exe",
        "vpnkit.exe",
    ] {
        let mut kill = Command::new("taskkill");
        kill.args(["/F", "/IM", process]);
        let _ = run_hidden_output(kill);
    }

    thread::sleep(Duration::from_secs(3));

    Ok(())
}

#[tauri::command]
async fn cleanup_docker() -> Result<ActionProgress, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cleanup_leftover_docker()?;

        Ok(ActionProgress {
            step: "docker-cleaned".to_string(),
            message: "Servicios Docker previamente activos se limpiaron".to_string(),
        })
    })
    .await
    .map_err(|error| format!("No se pudo limpiar Docker: {error}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![start_docker_desktop, wait_docker_ready, check_docker_ready, probe_docker_startup, start_compose, stop_compose, shutdown_docker_desktop, wait_docker_stopped, snapshot_docker_processes, cleanup_docker])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
