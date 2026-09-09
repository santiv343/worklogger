# Estado operativo

- Fecha UTC: 2026-09-09
- Rama: `main`
- Objetivo: permitir instalación MCP manual y no interactiva, documentada para personas y asistentes.

## En curso

- [-] Verificar el release `0.7.9`, actualizar el estado operativo y preparar el push.

## Decisiones recientes

- La configuración manual reutiliza el schema local `mcp.json`; los tokens sólo se aceptan desde el almacén seguro o variables de entorno.
- La TUI sigue siendo el flujo guiado; el modo headless requiere `--yes` explícito antes de modificar configuración de clientes.
- La documentación pública usa inglés; la TUI conserva español como idioma de interfaz.
- `--skills` se documenta como una operación separada de `--clients` porque instala en todos los destinos de skills compatibles detectados.

## Riesgos

- Los registros MCP ajenos o inválidos no se reemplazan en modo no interactivo.

## Validaciones

- `cargo +1.88.0 fmt --all --check`
- `cargo +1.88.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.88.0 test --workspace --locked --offline` (pruebas live sin credenciales: ignoradas)
- `npm test` en `packages/setup`
- `cargo +1.88.0 run -p worklogger-mcp -- --help`

## Próxima acción

- Crear el commit y tag de release `v0.7.9`, luego verificar la publicación npm.
