# Worklogger

Aplicación de escritorio modular para simplificar trabajo cotidiano conectado a
herramientas como Jira. La distribución base no contiene nombres, logos,
tableros, URLs ni credenciales de ninguna organización.

La versión actual permite:

- consultar, crear, editar y eliminar horas propias en Jira;
- navegar períodos y ver carga diaria, tareas y progreso semanal;
- descubrir los tableros accesibles para la cuenta autenticada;
- consultar reportes personales y, con permisos, reportes de equipo;
- exportar reportes a XLSX y PDF con trazabilidad a Jira;
- exponer capacidades seleccionadas mediante un servidor MCP standalone;
- instalar o quitar ese servidor en clientes MCP detectados;
- aplicar branding, límites y políticas mediante un perfil de organización.

## Ediciones

El mismo código genera dos clases de distribución:

| Edición | Configuración organizacional | Uso |
|---|---|---|
| Community | Manual o importada/exportada como JSON | Producto genérico y desarrollo |
| Managed | Perfil JSON embebido durante el build e inmutable | Distribución preparada por una organización |

Una edición Managed no compila la interfaz de importación/exportación de
configuración y no consulta archivos externos al ejecutarse. Las preferencias
personales y las credenciales siguen siendo privadas para cada usuario.

Los perfiles nunca contienen API tokens. Jira determina la identidad y los
permisos efectivos; Worklogger nunca permite administrar horas de terceros.

## Configuración modular compartida

`organization.json` es el único archivo compartible de Worklogger. Contiene
branding y una sección opcional por módulo bajo `modules`; si una sección no
existe, la organización no ofrece ese módulo. El archivo puede incluir ámbitos,
límites y capacidades, pero nunca identidad, correo, tokens ni estado de sesión.

La disponibilidad final de una función exige que el addon esté compilado, que
su sección exista en el perfil, que el usuario la habilite y que la cuenta
autenticada tenga permiso dentro del ámbito configurado. Ninguna configuración
local puede ampliar los permisos del proveedor.

Desktop Community y la TUI/MCP usan el mismo perfil instalado en el directorio
de configuración del usuario (`%APPDATA%\Worklogger` en Windows o
`$XDG_CONFIG_HOME/worklogger`/`~/.config/worklogger` en Linux). Así, una persona
puede compartir el archivo con el equipo y cada integrante sólo completa sus
propias credenciales.
La edición Managed embebe ese mismo schema durante el build y lo mantiene
inmutable tanto en Desktop como en el sidecar MCP.

El ámbito de cada proveedor es explícito: `restricted` requiere sitios o
workspaces permitidos y `unrestricted` requiere una lista vacía deliberada. Las
capacidades y campos `maximumAllowed*` del perfil son máximos organizacionales;
los otros límites son defaults y el usuario puede elegir valores menores. El
Desktop actual requiere Jira porque incluye Horas como experiencia principal,
mientras Reportes puede omitirse. El MCP standalone admite combinaciones
sólo-Jira, sólo-Bitbucket o sin addons.

## Ejecutar en desarrollo

Requiere Rust 1.88 y Dioxus CLI 0.7.9:

```bash
rustup toolchain install 1.88.0 --profile minimal --component clippy,rustfmt
cargo install dioxus-cli --version 0.7.9 --locked
./dev-desktop
```

Para usar un perfil local sin incorporarlo al repositorio:

```bash
cp config/example.organization.json config/worklogger.local.json
./dev-desktop
```

`dev-desktop` compila también el servidor MCP y deja su ruta disponible para la
pantalla Configuración → MCP. Los cambios de código continúan usando hot reload.

## MCP opcional

El servidor `worklogger-mcp` es independiente de Desktop y opera siempre como
las cuentas autenticadas de cada proveedor. Jira es un módulo único: Issues y
Horas son grupos de capacidades internos que comparten conexión y ámbito, pero
mantienen servicios separados. Bitbucket es otro módulo y puede compilarse sin
Jira. Horas nunca acepta una persona como parámetro ni permite modificar horas
ajenas.

El catálogo actual incluye:

- Jira Cloud: detalle y búsqueda JQL de issues, metadata editable,
  transiciones, edición de campos y comentarios;
- Horas en Jira: consulta y carga confirmada de horas propias, más detección de
  issues del sprint asignados a la cuenta autenticada sin worklogs propios en
  el período;
- Bitbucket Cloud: repositorios, listado/detalle/actividad de pull requests,
  creación, edición, comentarios, aprobación, solicitud de cambios, merge y
  decline.

Cada grupo tiene una capacidad propia. Merge y decline permanecen separados de
edición y review; desactivar una capacidad retira sus tools del handshake. Toda
escritura primero devuelve una vista previa con actor, destino, efecto y un token
de un solo uso. Sólo se ejecuta al reenviar el mismo pedido con `confirmed: true`
y ese token; el estado incluido en la vista previa se vuelve a consultar y, si
cambió, la confirmación deja de ser válida. En toda mutación de un PR se
comparan otra vez estado, ramas, hashes y revisión inmediatamente antes de
escribir. Los sitios,
workspaces, repositorios, ramas, estados, reviewers y campos no están
predefinidos para ninguna empresa.

`jira_create_worklog` recibe issue, instante RFC 3339 con el desfase configurado,
duración entera en minutos y comentario opcional. No acepta autor. La vista
previa informa coincidencias propias del mismo issue, fecha y duración para
evitar duplicados accidentales. `jira_search_issues` devuelve como máximo el
límite pedido y señala con `hasMore` si existe otra página.

La detección de tareas sin horas funciona sólo con Jira. Si Bitbucket está
instalado y habilitado, sus pull requests y actividad pueden aportar evidencia
adicional, pero nunca son un requisito para calcular los candidatos de Jira.

Desde Desktop, Configuración → MCP permite activar capacidades y registrar o
quitar Worklogger en los clientes detectados. Antes de modificar un cliente se
muestran el destino y el alcance exactos y se solicita confirmación.

El paquete es público y no requiere cuenta ni token de Worklogger. Abrí la TUI
desde PowerShell, Linux o WSL:

```shell
npx @santiv343/worklogger
```

Si configuraste previamente este scope para GitHub Packages, ejecutá una vez
`npm config delete @santiv343:registry` para volver al registry público de npm.

Para instalar de una vez el perfil compartido del equipo y abrir el asistente:

```shell
# Windows
npx @santiv343/worklogger setup --profile C:\ruta\organization.json

# Linux o WSL
npx @santiv343/worklogger setup --profile /ruta/organization.json
```

Comandos disponibles:

```shell
npx @santiv343/worklogger clients
npx @santiv343/worklogger skills
npx @santiv343/worklogger status
npx @santiv343/worklogger uninstall
```

La TUI permite configurar Jira o Bitbucket y elegir sus capacidades por
separado. En Jira, Horas e Issues aparecen como grupos internos del mismo
módulo; al activar Horas solicita el objetivo semanal y el desfase horario. Si
se vuelve a ejecutar el asistente, propone los valores actuales y conserva la
configuración del otro proveedor y los demás workspaces Bitbucket de la misma
cuenta. Desktop permite administrar las capacidades ya configuradas y sólo
registra clientes cuando están disponibles las credenciales de todos los
proveedores activos.

`organization.json` es la fuente compartible de módulos, ámbitos máximos,
límites y capacidades permitidas. `mcp.json` conserva únicamente el estado
local resuelto del servidor MCP —incluido el correo de esa persona— y nunca se
comparte. `config.json` conserva preferencias y conexión locales de Desktop.
Los tokens permanecen fuera de los JSON: Windows usa Credential Manager y Linux
un almacén atómico del usuario protegido con permisos `0700/0600`. Cambiar una
capacidad desde Desktop preserva siempre el ámbito ya elegido para MCP; un cambio
explícito de conexión en Desktop vuelve a sincronizarlo sin superar el perfil.

El comando `skills` instala o actualiza las skills de flujo propias de
Worklogger para los asistentes compatibles. Sólo administra las carpetas
`worklogger-jira`, `worklogger-daily` y `worklogger-delivery`; si encuentra una
de esas carpetas que no fue creada por Worklogger, no la reemplaza.

Para probar el catálogo completo durante desarrollo se puede partir de
[`config/example.mcp.json`](config/example.mcp.json), indicar su ruta y pasar
los secretos sólo por variables de entorno:

```powershell
$env:WORKLOGGER_MCP_CONFIG = "C:\ruta\mcp.json"
$env:WORKLOGGER_JIRA_API_TOKEN = "<token-jira>"
$env:WORKLOGGER_BITBUCKET_API_TOKEN = "<token-bitbucket>"
worklogger-mcp serve
```

Bitbucket usa API tokens con scopes, correo de la cuenta Atlassian y el endpoint
oficial de Bitbucket Cloud. No permite reemplazar el endpoint por una URL que
pueda recibir credenciales. Jira Data Center y Bitbucket Data Center requieren
addons diferentes.

Actualmente se detectan Codex, Claude Code, Claude Desktop, Cursor y Windsurf.
El propio binario se instala en una ruta versionada: bajo
`%LOCALAPPDATA%\Worklogger\MCP` en Windows y `$XDG_DATA_HOME/worklogger/MCP` o
`~/.local/share/worklogger/MCP` en Linux/WSL. Desktop y la TUI registran la ruta
nativa correspondiente. Cada versión queda aislada para no romper clientes en
uso. Node no queda como dependencia de ejecución.

El instalador debe ejecutarse en el mismo entorno que el cliente: dentro de WSL
para Codex o Claude Code instalados allí, y desde PowerShell para aplicaciones
Windows. Ninguna interoperabilidad WSL/Windows es necesaria para que el servidor
funcione. La distribución Linux x64 se compila y prueba sobre Ubuntu 22.04, con
glibc 2.35 como base mínima soportada.

Después de actualizar el servidor, cada cliente MCP debe reiniciarse para cerrar
el proceso de la versión anterior y comenzar un handshake con la ruta nueva.

Al actualizar, Worklogger distingue un registro propio anterior de un conflicto
ajeno: el primero se puede reparar o quitar; el segundo nunca se elimina sin una
confirmación explícita de reemplazo. También reconoce la instalación habitual
de Codex mediante npm en Windows (`codex.cmd`), pero ejecuta su entrypoint con
Node directamente para no delegar argumentos a un shell. Ningún archivo de
cliente recibe tokens de Jira ni de Bitbucket.

## Verificación

```bash
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo check --package worklogger-desktop --no-default-features --locked
cargo check --package worklogger-desktop --no-default-features --features hours --locked
```

## Crear distribuciones Windows

Desde PowerShell:

```powershell
# Edición libre
.\scripts\build-windows.ps1 -Edition Community

# Sólo Jira en Desktop y MCP
.\scripts\build-windows.ps1 -Edition Community -McpAddons jira

# Edición corporativa inmutable
.\scripts\build-windows.ps1 `
  -Edition Managed `
  -Profile C:\profiles\company.json `
  -Name Company
```

Para generar ambas en una sola ejecución:

```powershell
.\scripts\build-release-set.ps1 `
  -ManagedProfile C:\profiles\company.json `
  -ManagedName Company
```

Cada ejecución deja en `dist/` el instalador, el ZIP portable y su SHA-256. La
guía completa está en
[Distribuciones personalizadas](docs/architecture/custom-distributions.md).

## Documentación

- [Guía de uso de Desktop y MCP](docs/user-guide/README.md)
- [Changelog](CHANGELOG.md)
- [Arquitectura modular](docs/architecture/modularity.md)
- [Tools MCP de Jira y Bitbucket](docs/architecture/mcp-provider-tools.md)
- [Distribuciones personalizadas](docs/architecture/custom-distributions.md)
- [Permisos y límites de confianza](docs/security/permission-model.md)
- [ADR: una base de código, múltiples ediciones](docs/adr/0001-single-codebase-distributions.md)
- [ADR: neutralidad por cortes verticales](docs/adr/0002-provider-neutral-vertical-slices.md)
- [ADR: mutaciones MCP nativas por proveedor](docs/adr/0003-provider-native-mcp-mutations.md)
- [ADR: perfil organizacional modular](docs/adr/0004-modular-organization-profile.md)
- [Versionado y releases](docs/architecture/versioning-and-releases.md)

## Tecnología

- Rust 2024
- Dioxus Desktop 0.7
- WebView2 en Windows
- Jira Cloud REST API
- Bitbucket Cloud REST API
- Windows Credential Manager o almacén Linux privado para secretos

El paquete de bootstrap se distribuye públicamente por npm. Los tags publican
los servidores MCP Windows/Linux junto con ese paquete; los instaladores Desktop
Windows quedan como artifacts del workflow hasta que se cree una release
explícita.
