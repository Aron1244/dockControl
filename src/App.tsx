import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Check, CircleX, LoaderCircle, MonitorSmartphone, Rocket, SquareTerminal } from "lucide-react";
import "./App.css";

const STORAGE_KEY = "dockcontrol.paths";

function ActivityBar({
  status,
  progress,
  loading,
  phase,
}: {
  status: string;
  progress: number;
  loading: "start" | "stop" | null;
  phase: "idle" | "busy" | "returning" | "completing";
}) {
  const [showCompletion, setShowCompletion] = useState(false);
  const prevPhaseRef = useRef(phase);

  useEffect(() => {
    if (phase === "completing") {
      setShowCompletion(true);
    } else if (phase === "idle" && prevPhaseRef.current === "completing") {
      const timer = setTimeout(() => setShowCompletion(false), 350);
      prevPhaseRef.current = phase;
      return () => clearTimeout(timer);
    } else {
      setShowCompletion(false);
    }
    prevPhaseRef.current = phase;
  }, [phase]);

  return (
    <div className={`activity-bar-shell activity-bar-${phase}`} aria-live="polite">
      <div className={`activity-bar ${showCompletion ? "is-completing" : ""}`}>
        {showCompletion ? (
          <Check className="check-icon" />
        ) : (
          <>
            <div className={`activity-dot ${loading ? "is-active" : ""}`} />
            <div className="activity-copy">
              <strong>{loading ? (loading === "start" ? "Arrancando" : "Deteniendo") : "Listo"}</strong>
              <span>{status}</span>
            </div>
            <div className="activity-progress" aria-label={`Progreso ${progress}%`}>
              <span style={{ width: `${progress}%` }} />
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function App() {
  const logEndRef = useRef<HTMLDivElement | null>(null);
  const [dockerDesktopPath, setDockerDesktopPath] = useState(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) return "C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe";

    try {
      const parsed = JSON.parse(stored) as { dockerDesktopPath?: string };
      return parsed.dockerDesktopPath ?? "C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe";
    } catch {
      return "C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe";
    }
  });
  const [composeDir, setComposeDir] = useState(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) return "";

    try {
      const parsed = JSON.parse(stored) as { composeDir?: string };
      return parsed.composeDir ?? "";
    } catch {
      return "";
    }
  });
  const [status, setStatus] = useState("Listo para configurar.");
  const [loading, setLoading] = useState<"start" | "stop" | null>(null);
  const [activityPhase, setActivityPhase] = useState<"idle" | "busy" | "returning" | "completing">("idle");
  type Step = { id: string; label: string; detail: string; state: "idle" | "running" | "done" | "error" };
  const [steps, setSteps] = useState<Step[]>([]);

  function setStepState(id: string, state: Step["state"]) {
    setSteps((current) => current.map((step) =>
      step.id === id ? { ...step, state } : step
    ));
  }
  const [logs, setLogs] = useState<string[]>([]);
  const isBusy = loading !== null;

  const progress = useMemo(() => {
    if (!steps.length) return 0;
    const done = steps.filter((step) => step.state === "done").length;
    return Math.round((done / steps.length) * 100);
  }, [steps]);

  const currentStep = steps.find((step) => step.state === "running") ?? null;
  const doneCount = steps.filter((step) => step.state === "done").length;

  useEffect(() => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ dockerDesktopPath, composeDir }),
    );
  }, [dockerDesktopPath, composeDir]);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [logs]);

  useEffect(() => {
    document.body.classList.toggle("busy-mode", isBusy);

    return () => {
      document.body.classList.remove("busy-mode");
    };
  }, [isBusy]);

  useEffect(() => {
    if (loading) {
      setActivityPhase("busy");
      return;
    }

    if (activityPhase === "busy") {
      setActivityPhase("returning");
    }
  }, [loading]);

  useEffect(() => {
    if (activityPhase === "returning") {
      const timer = window.setTimeout(() => setActivityPhase("completing"), 300);
      return () => window.clearTimeout(timer);
    }

    if (activityPhase === "completing") {
      const timer = window.setTimeout(() => setActivityPhase("idle"), 1500);
      return () => window.clearTimeout(timer);
    }
  }, [activityPhase]);

  function pushLog(line: string) {
    setLogs((current) => [...current, line]);
  }

  async function startStack() {
    setLoading("start");
    setStatus("Iniciando Docker Desktop y el contenedor...");
    setLogs([]);
    setSteps([
      { id: "cleanup", label: "Limpiar servicios Docker", detail: "taskkill / sc stop com.docker.service / wsl --shutdown", state: "idle" },
      { id: "start-docker", label: "Abrir Docker Desktop", detail: dockerDesktopPath, state: "idle" },
      { id: "wait-ready", label: "Esperar Docker listo", detail: "docker info", state: "idle" },
      { id: "compose-up", label: "Levantar contenedor", detail: `docker compose up -d (${composeDir})`, state: "idle" },
    ]);

    try {
      setStepState("cleanup", "running");
      pushLog(`> cleanup_docker`);
      const cleaned = await invoke<{ step: string; message: string }>("cleanup_docker");
      setStatus(cleaned.message);
      pushLog(`< ${cleaned.message}`);
      setStepState("cleanup", "done");

      setStepState("start-docker", "running");
      pushLog(`> start_docker_desktop ${dockerDesktopPath}`);
      const dockerStarted = await invoke<{ step: string; message: string }>(
        "start_docker_desktop",
        { dockerDesktopPath },
      );
      setStatus(dockerStarted.message);
      pushLog(`< ${dockerStarted.message}`);
      setStepState("start-docker", "done");

      setStepState("wait-ready", "running");
      let dockerReady = false;
      for (let attempt = 0; attempt < 60; attempt += 1) {
        pushLog(`> probe_docker_startup attempt ${attempt + 1}`);
        const check = await invoke<{ ready: boolean; message: string; processes: string[] }>(
          "probe_docker_startup",
        );
        setStatus(check.message);
        pushLog(`< ${check.message}`);
        if (check.processes.length) {
          pushLog(`> procesos: ${check.processes.join(" | ")}`);
        }

        if (check.ready) {
          dockerReady = true;
          break;
        }

        await new Promise((resolve) => setTimeout(resolve, 2000));
      }

      if (!dockerReady) {
        throw new Error("Docker Engine no quedó listo a tiempo");
      }

      setStepState("wait-ready", "done");

      setStepState("compose-up", "running");
      pushLog(`> start_compose ${composeDir}`);
      const composeStarted = await invoke<{ step: string; message: string }>(
        "start_compose",
        { composeDir },
      );

      setStatus(composeStarted.message);
      pushLog(`< ${composeStarted.message}`);
      setStepState("compose-up", "done");
    } catch (error) {
      setStatus(`Error: ${String(error)}`);
      pushLog(`! Error: ${String(error)}`);
      setSteps((current) => current.map((step) => (step.state === "running" ? { ...step, state: "error" } : step)));
    } finally {
      setLoading(null);
    }
  }

  async function pickDockerDesktop() {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Docker Desktop", extensions: ["exe"] }],
    });

    if (typeof selected === "string") {
      setDockerDesktopPath(selected);
    }
  }

  async function pickComposeDir() {
    const selected = await open({
      multiple: false,
      directory: true,
    });

    if (typeof selected === "string") {
      setComposeDir(selected);
    }
  }

  async function stopStack() {
    setLoading("stop");
    setStatus("Deteniendo el contenedor y cerrando Docker Desktop...");
    setLogs([]);
    setSteps([
      { id: "compose-down", label: "Bajar contenedor", detail: `docker compose down (${composeDir})`, state: "idle" },
      { id: "shutdown", label: "Cerrar Docker Desktop", detail: "DockerCli -Shutdown / wsl --shutdown / sc stop com.docker.service", state: "idle" },
      { id: "wait-close", label: "Esperar cierre", detail: "docker info + tasklist + sc query", state: "idle" },
    ]);

    try {
      setStepState("compose-down", "running");
      pushLog(`> stop_compose ${composeDir}`);
      const composeStopped = await invoke<{ step: string; message: string }>(
        "stop_compose",
        { composeDir },
      );
      setStatus(composeStopped.message);
      pushLog(`< ${composeStopped.message}`);
      setStepState("compose-down", "done");

      setStepState("shutdown", "running");
      pushLog(`> shutdown_docker_desktop ${dockerDesktopPath}`);
      const shutdownRequested = await invoke<{ step: string; message: string }>(
        "shutdown_docker_desktop",
        { dockerDesktopPath },
      );
      setStatus(shutdownRequested.message);
      pushLog(`< ${shutdownRequested.message}`);
      setStepState("shutdown", "done");

      setStepState("wait-close", "running");
      pushLog(`> wait_docker_stopped`);
      const dockerStopped = await invoke<{ step: string; message: string }>(
        "wait_docker_stopped",
      );
      setStatus(dockerStopped.message);
      pushLog(`< ${dockerStopped.message}`);
      pushLog(`> docker engine confirmed stopped`);
      const snapshot = await invoke<{ processes: string[] }>("snapshot_docker_processes");
      pushLog(`> docker process snapshot: ${snapshot.processes.length ? snapshot.processes.join(" | ") : "none"}`);
      setStepState("wait-close", "done");
    } catch (error) {
      setStatus(`Error: ${String(error)}`);
      pushLog(`! Error: ${String(error)}`);
      setSteps((current) => current.map((step) => (step.state === "running" ? { ...step, state: "error" } : step)));
    } finally {
      setLoading(null);
    }
  }

  return (
    <main className="app-shell">
      <ActivityBar status={status} progress={progress} loading={loading} phase={activityPhase} />
      <section className="panel">
        <p className="eyebrow">
          <MonitorSmartphone className="eyebrow-icon" aria-hidden="true" />
          dockControl
        </p>
        <h1>Arranca y detiene tu stack con un clic</h1>
        <p className="subtitle">
          Configura la ruta de Docker Desktop y la carpeta donde está tu
          `docker-compose.yml`.
        </p>

        <div className="workspace-grid">
          <section className="card controls-card">
            <div className="card-header">
              <div>
                <h2>Configuración</h2>
                <p>Rutas locales y acción principal.</p>
              </div>
              <span className={`status-chip ${isBusy ? "status-chip-busy" : "status-chip-ready"}`}>
                {isBusy ? "En curso" : "Listo"}
              </span>
            </div>

            <label>
              Ruta de Docker Desktop
              <div className="field-row">
                <input
                  value={dockerDesktopPath}
                  onChange={(e) => setDockerDesktopPath(e.currentTarget.value)}
                  placeholder="C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe"
                />
                <button type="button" className="ghost" onClick={pickDockerDesktop}>
                  Buscar
                </button>
              </div>
            </label>

            <label>
              Carpeta del proyecto Docker
              <div className="field-row">
                <input
                  value={composeDir}
                  onChange={(e) => setComposeDir(e.currentTarget.value)}
                  placeholder="C:\\ruta\\de\\tu\\proyecto"
                />
                <button type="button" className="ghost" onClick={pickComposeDir}>
                  Buscar
                </button>
              </div>
            </label>

            <div className="actions actions-stack">
              <button onClick={startStack} disabled={loading !== null || !composeDir}>
                {loading === "start" ? <LoaderCircle className="button-icon spinner-icon" aria-hidden="true" /> : <Rocket className="button-icon" aria-hidden="true" />}
                <span>{loading === "start" ? "Iniciando..." : "Levantar stack"}</span>
              </button>
              <button className="secondary" onClick={stopStack} disabled={loading !== null || !composeDir}>
                <SquareTerminal className="button-icon" aria-hidden="true" />
                <span>{loading === "stop" ? "Deteniendo..." : "Cerrar stack"}</span>
              </button>
            </div>
          </section>

          <aside className="card summary-card">
            <div className="card-header">
              <div>
                <h2>Estado</h2>
                <p>Resumen visual del flujo.</p>
              </div>
            </div>

            <div className="summary-kpis">
              <div>
                <span>Progreso</span>
                <strong>{progress}%</strong>
              </div>
              <div>
                <span>Completados</span>
                <strong>{doneCount}/{steps.length || 3}</strong>
              </div>
            </div>

            <div className="summary-current">
              <span className="summary-label">Paso actual</span>
              <strong>{currentStep?.label ?? "Sin ejecución"}</strong>
              <p>{currentStep?.detail ?? status}</p>
            </div>

            <div className="summary-tags">
              <span>{composeDir ? "Compose configurado" : "Compose pendiente"}</span>
              <span>{dockerDesktopPath ? "Docker Desktop listo" : "Ruta faltante"}</span>
            </div>
          </aside>
        </div>

        <section className="card timeline-card">
          <div className="card-header">
            <div>
              <h2>Pasos en ejecución</h2>
              <p>Animación y estado en tiempo real.</p>
            </div>
          </div>
          <ul className="step-list">
            {steps.length === 0 ? (
              <li className="step-empty">Sin acciones en curso.</li>
            ) : (
              steps.map((step, index) => (
                <li key={step.id} className={`step step-${step.state}`}>
                  <span className="step-index">
                    {step.state === "done" ? (
                      <Check className="step-icon" aria-hidden="true" />
                    ) : step.state === "error" ? (
                      <CircleX className="step-icon" aria-hidden="true" />
                    ) : step.state === "running" ? (
                      <LoaderCircle className="step-icon spinner-icon" aria-hidden="true" />
                    ) : (
                      <span>{`0${index + 1}`}</span>
                    )}
                  </span>
                  <div className="step-body">
                    <span className="step-label">{step.label}</span>
                    <span className="step-detail">{step.detail}</span>
                  </div>
                  <span className={`step-pill step-pill-${step.state}`}>{step.state}</span>
                </li>
              ))
            )}
          </ul>
        </section>

        <section className="card terminal-card">
          <div className="card-header">
            <div>
              <h2>Comandos</h2>
              <p>Salida breve de cada acción.</p>
            </div>
          </div>
          <pre className="terminal-log">{logs.length ? logs.join("\n") : "Sin comandos ejecutados."}</pre>
          <div ref={logEndRef} />
        </section>
      </section>
    </main>
  );
}

export default App;
