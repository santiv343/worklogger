# Distribuciones personalizadas

## Modelo

Una sola base de código genera:

- `Community`: configuración organizacional mutable;
- `Managed`: perfil organizacional embebido e inmutable.

Ambas pueden incluir `jira`, `bitbucket`, ambos addons MCP o ninguno. La
selección se realiza en build time y el código del addon excluido no forma parte
de los binarios.

No se crea un fork por empresa. Cada organización mantiene fuera del repo un
JSON sin secretos y genera su instalador contra una versión concreta.

## Perfil

Copiar `config/example.organization.json` y modificar únicamente valores
explícitos. El perfil puede definir:

- nombre, logo y colores;
- módulos disponibles bajo `modules`;
- sitios y tableros Jira sugeridos o permitidos;
- workspaces y repositorios Bitbucket permitidos;
- capacidades MCP máximas por proveedor;
- límites operativos;
- defaults de Horas;
- política de acceso a reportes de equipo;
- límites visuales y de exportación.

Cada proveedor declara `scopeMode`. `restricted` requiere recursos permitidos;
`unrestricted` requiere un ámbito vacío y habilita la selección dentro de todo
lo que permita la cuenta autenticada. Las capacidades y campos
`maximumAllowed*` son máximos: la configuración local sólo puede reducirlos.
Los límites sin ese prefijo son los valores sugeridos para conexiones nuevas.

Nunca debe contener correos personales, API tokens ni otros secretos.
Una sección de módulo ausente lo deshabilita para esa organización; una sección
presente no incorpora el addon si el binario fue compilado sin él.

## Community

```powershell
.\scripts\build-windows.ps1 -Edition Community
```

Incluye `configurable-organization`; el usuario puede completar la configuración
manualmente o importar/exportar un JSON validado.

Para acotar los addons MCP compilados:

```powershell
.\scripts\build-windows.ps1 -Edition Community -McpAddons jira
.\scripts\build-windows.ps1 -Edition Community -McpAddons bitbucket
.\scripts\build-windows.ps1 -Edition Community -McpAddons @()
```

Sin `-McpAddons`, se incluyen Jira y Bitbucket. Esta selección no habilita
capacidades por sí sola: cada usuario las configura después según sus permisos.

## Managed

```powershell
.\scripts\build-windows.ps1 `
  -Edition Managed `
  -Profile C:\profiles\company.json `
  -Name Company
```

El build:

1. valida que el archivo exista y sea JSON;
2. compila `managed-distribution` sin `configurable-organization`;
3. incorpora el contenido al ejecutable;
4. valida siempre el schema embebido y, salvo con `-SkipChecks`, ejecuta formato,
   Clippy y todos los tests;
5. genera un instalador NSIS y un ZIP portable en `dist/`.

Al ejecutarse, la edición Managed no busca `WORKLOGGER_CONFIG`, un archivo junto
al `.exe` ni `%APPDATA%\Worklogger\organization.json`. Tampoco incorpora la UI
para seleccionar o exportar configuraciones. Esto no afecta la exportación de
reportes XLSX/PDF.

El mismo perfil se incorpora al sidecar `worklogger-mcp.exe`. Ni Desktop ni MCP
aceptan reemplazarlo mediante `--profile` o archivos locales. El Desktop actual
incluye Horas como experiencia principal y por eso requiere el módulo Jira; el
MCP standalone sí admite builds sólo-Jira, sólo-Bitbucket o sin addons. Reportes
puede omitirse y desaparece de la navegación.

## Generar las dos ediciones

```powershell
.\scripts\build-release-set.ps1 `
  -ManagedProfile C:\profiles\company.json `
  -ManagedName Company
```

El resultado usa nombres versionados:

```text
dist/Worklogger-Community-<version>-Setup.exe
dist/Worklogger-Community-<version>-Setup.exe.sha256
dist/Worklogger-Community-<version>-Portable.zip
dist/Worklogger-Community-<version>-Portable.zip.sha256
dist/Worklogger-Company-<version>-Setup.exe
dist/Worklogger-Company-<version>-Setup.exe.sha256
dist/Worklogger-Company-<version>-Portable.zip
dist/Worklogger-Company-<version>-Portable.zip.sha256
dist/Worklogger-Company-<version>-Profile.sha256
```

Cada ZIP portable conserva el ejecutable y sus assets en una única carpeta. No
requiere instalación ni permisos de administrador, pero usa WebView2 Evergreen,
la configuración de `%APPDATA%\Worklogger` y Windows Credential Manager. El
archivo `.sha256` contiguo permite verificar la descarga.
La edición Managed agrega el hash del perfil embebido sin publicar su contenido.

El repositorio público construye y publica únicamente la edición Community. Una
organización que necesite una edición Managed ejecuta el script de build desde
un pipeline privado, tomando un tag público como fuente y un perfil externo
que no se incorpora al repositorio ni a sus workflows. El nombre, branding y
comportamiento corporativo siguen viniendo exclusivamente del perfil embebido.

## Requisitos del equipo de build

- Windows 10/11;
- Rust 1.88;
- Dioxus CLI 0.7.9;
- Visual Studio Build Tools 2022 con `Desktop development with C++`;
- NSIS disponible para Dioxus.

El usuario final sólo necesita Windows y el instalador.

## Checklist antes de distribuir

- perfil revisado y sin secretos;
- tests Community y Managed verdes;
- instalador y portable ejecutados en un usuario limpio;
- onboarding validado con una cuenta sin permisos administrativos;
- identidad y permisos visibles después de conectar;
- firma de código aplicada cuando exista certificado;
- hash y versión del instalador registrados.
