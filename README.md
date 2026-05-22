# DockControl

Aplicación de escritorio para gestionar entornos Docker locales con un clic.

Construida con **Tauri v2 + React 19 + Rust**, ofrece una interfaz limpia para levantar y detener Docker Desktop junto con tu stack de `docker-compose.yml`, sin necesidad de usar la terminal.

![version](https://img.shields.io/badge/version-1.0.0-blue)

---

## Características

- Inicia Docker Desktop automáticamente y espera a que el engine esté listo.
- Ejecuta `docker compose up -d` con reintentos.
- Detiene el stack con `docker compose down` (timeout, remove orphans).
- Cierra Docker Desktop limpiamente: `DockerCli -Shutdown` + `wsl --shutdown`.
- Detiene el servicio `com.docker.service` de Windows.
- Limpieza previa de procesos residuales al iniciar.
- Animaciones de progreso con efecto de chispas en la barra y morph a check al completar.
- Logs en vivo de cada comando ejecutado.
- Persiste las rutas configuradas en localStorage.
- Instaladores MSI y NSIS para Windows x64.

---

## Stack tecnológico

| Capa | Tecnología |
|---|---|
| Frontend | React 19, TypeScript, Vite, Lucide React |
| Backend desktop | Tauri v2, Rust |
| Estilos | CSS vanilla con animaciones keyframe |
| Paquete | pnpm, tauri-cli |

---

## Requisitos

- Windows 10/11
- Docker Desktop instalado
- pnpm (para desarrollo)

---

## Desarrollo

```bash
pnpm install
pnpm tauri dev
```

---

## Build

```bash
pnpm tauri build
```

Genera instaladores en `src-tauri/target/release/bundle/`:
- `msi/dockControl_1.0.0_x64_en-US.msi`
- `nsis/dockControl_1.0.0_x64-setup.exe`

---

## Uso

1. Pega la ruta de `Docker Desktop.exe` o usa el botón **Buscar**.
2. Pega la carpeta donde está tu `docker-compose.yml` o búscala.
3. Presiona **Levantar stack** para iniciar.
4. Presiona **Cerrar stack** para detener.

---

## Licencia

MIT
