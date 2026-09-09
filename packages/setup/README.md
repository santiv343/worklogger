# Worklogger MCP setup

CLI pública para configurar y administrar el servidor MCP standalone en Windows
x64 o Linux x64 con glibc 2.35 o posterior, incluido Ubuntu 22.04+ y WSL2.
Incluye ambos binarios Rust y ejecuta el nativo del sistema. El servidor queda
instalado en una ruta versionada del usuario:

- Windows: `%LOCALAPPDATA%\Worklogger\MCP\<versión>`;
- Linux/WSL: `$XDG_DATA_HOME/worklogger/MCP/<versión>` o
  `~/.local/share/worklogger/MCP/<versión>`.

Node se usa sólo para abrir el instalador; no es necesario cuando el servidor
está funcionando.

La instalación no requiere cuenta ni token de Worklogger:

```shell
npx @santiv343/worklogger
```

Si antes configuraste este scope para GitHub Packages, volvé al registry público
de npm una sola vez:

```shell
npm config delete @santiv343:registry
```

Sin argumentos abre la TUI principal. Desde ahí se consultan el estado, la ruta
instalada, los módulos activos y los clientes detectados; también se accede a la
configuración y a la instalación o retiro en cada cliente.

Para reutilizar la configuración modular de un equipo:

```shell
# Windows
npx @santiv343/worklogger setup --profile C:\ruta\organization.json

# Linux o WSL
npx @santiv343/worklogger setup --profile /ruta/organization.json
```

El perfil se valida y se instala bajo el directorio de configuración del
usuario. Puede definir branding, módulos, ámbitos, límites y capacidades
permitidas, pero no debe contener cuentas ni secretos. Cada usuario ingresa su
propio correo y token durante el asistente. El token queda en Windows Credential
Manager o, en Linux, en un almacén local atómico con permisos `0700/0600`; nunca
se copia al JSON ni a los clientes MCP.

También acepta:

```shell
npx @santiv343/worklogger clients
npx @santiv343/worklogger status
npx @santiv343/worklogger uninstall
```

`setup` ofrece únicamente los módulos incluidos en el binario y permitidos por
el perfil, verifica la cuenta, descubre recursos dentro del ámbito y ofrece
instalar Worklogger en los clientes detectados. `clients` permite instalarlo o
quitarlo después sin repetir el onboarding. Cada cambio muestra el archivo
afectado y pide confirmación.

`uninstall` retira únicamente los registros pertenecientes a esa instalación,
su configuración y su credencial MCP. No toca la credencial de Desktop ni borra
binarios versionados que podrían seguir abiertos.

Una versión nueva reconoce registros versionados anteriores, incluso cuando el
binario viejo ya no existe, y permite actualizarlos o quitarlos. Las entradas
homónimas ajenas se muestran como conflicto y se preservan al desinstalar.

Se soportan Codex, Claude Code, Claude Desktop, Cursor y Windsurf cuando están
disponibles en el mismo sistema donde se ejecuta el instalador. Para un cliente
que corre dentro de WSL, ejecutá `npx` dentro de esa distribución; para una app
Windows, ejecutalo desde PowerShell. El paquete se distribuye públicamente
desde npm. Cada persona configura sus propias credenciales de Jira o Bitbucket
durante el onboarding.
