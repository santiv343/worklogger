# Arquitectura modular

## Objetivo

Permitir distribuciones pequeñas y configurables sin duplicar negocio entre
Desktop, CLI, MCP o una futura web. Los addons son estáticos y conocidos al
compilar; su disponibilidad y exposición se deciden en runtime.

## Mapa de dependencias

```text
Desktop · CLI · MCP · futura API web
                 │
                 ▼
          casos de uso tipados
      ┌──────────┼───────────┐
      ▼          ▼           ▼
 módulo Jira   Reportes   módulo Bitbucket
  ├─ Issues
  └─ Horas
      │          │           │
      └──────────┴── ports ──┘
                         │
                         ▼
       Jira Cloud · Bitbucket Cloud · keyring · filesystem
```

`Jira` es un solo módulo instalable. `Issues` y `Horas` son grupos de
capacidades internos: comparten conexión, identidad autenticada, tablero y
política, aunque sus casos de uso permanezcan separados. `Bitbucket` es un
módulo independiente. `Reportes` consume lecturas habilitadas y nunca amplía
permisos de un proveedor.

Las superficies nunca llaman directamente a clientes REST. Cada caso de uso
define el port mínimo que necesita y recibe un adaptador en el composition root.

## MCP standalone

MCP se distribuye como un binario propio, sin WebView2 ni dependencia de la
aplicación Desktop. `worklogger-mcp` reutiliza los crates de dominio, casos de
uso y adaptadores; agrega el transporte MCP por `stdio` y su composition root.

La configuración inicial pertenece al mismo ejecutable y guarda el token en
el almacén seguro del sistema. La lista efectiva de tools deriva de las
features compiladas, el perfil de organización, la configuración del usuario y
los permisos otorgados por el proveedor. Instalar Desktop nunca es un requisito
para usar MCP.

Horas consulta exclusivamente las cargas de la identidad autenticada. Las
mutaciones de Issues y pull requests requieren preview y confirmación explícita;
las horas de otra persona siguen siendo de sólo lectura cuando existe
autorización para reportes.

### Instalación y configuración

Sin argumentos, el binario abre la TUI principal con el estado, las rutas, los
módulos y los clientes detectados. Los comandos `serve`, `setup`, `clients`,
`status` y `uninstall` conservan los mismos casos de uso para automatización. El
asistente de `setup` acepta `--profile <organization.json>`, instala ese perfil
compartido, muestra el archivo que va a modificar y pide confirmación antes de
registrar el servidor.

Hay dos entradas al mismo caso de uso:

1. Desktop lo presentará en Configuración → MCP y permitirá instalar, quitar y
   elegir módulos y capacidades disponibles.
2. `npx @santiv343/worklogger` ejecuta el binario incluido, que se instala
   en una ruta estable del usuario y abre su TUI. Node es sólo el bootstrap; el
   servidor no depende de Node.

La instalación es idempotente: una segunda ejecución actualiza o repara
el registro existente sin duplicarlo. Un registro versionado anterior sigue
siendo propiedad de Worklogger aunque falte su binario y puede repararse o
quitarse; una entrada homónima de otro origen continúa tratándose como conflicto.
La ubicación del binario dependerá del
usuario y del sistema operativo; nunca se escribirá un token dentro de la
configuración del cliente MCP. Desktop y la TUI convergen en una ruta versionada
por usuario; mover un portable no rompe los registros ya creados.

El catálogo inicial contiene Codex, Claude Code, Claude Desktop, Cursor y
Windsurf. Codex se modifica mediante su CLI oficial. En Windows, la instalación
npm habitual se resuelve desde `codex.cmd` hacia Node y `codex.js` sin ejecutar
el shim mediante un shell. Los clientes JSON reciben
únicamente la entrada `mcpServers.worklogger`, conservando las demás claves. Las
escrituras son atómicas, rechazan enlaces simbólicos y comprueban que el
documento no haya cambiado antes de escribir. Una configuración inválida falla
cerrada sin ser reemplazada. Los procesos de clientes tienen timeout configurable
y Desktop los consulta fuera del hilo de UI.

La selección visual comienza por módulos y permite reducir capacidades dentro
de cada uno. Dentro de Jira, Horas e Issues se presentan como grupos, nunca como
addons independientes. La configuración nunca puede ampliar lo compilado ni
los permisos efectivos. Por ejemplo, habilitar Jira/Horas no concede acceso a
horas ajenas ni convierte un reporte de equipo en una mutación.

## Contextos

| Contexto | Responsabilidad | No conoce |
|---|---|---|
| Platform Core | addons, capacidades, configuración y ámbitos | Jira, HTTP, Dioxus |
| Jira / conexión | cuenta, tableros, ámbito y permisos compartidos | reglas de Horas, UI |
| Jira / Issues | issues, campos, comentarios y transiciones | reglas de Horas, UI |
| Jira / Horas | rangos, duraciones, worklogs propios y resúmenes | Dioxus |
| Reportes | filtros, agregaciones y read models personal/equipo | mutaciones, tokens |
| Bitbucket | repositorios, PRs, reviews, tareas y pipelines | Jira, Horas |
| Work Hub | consultas cruzadas y señales explicables | clientes HTTP concretos |
| Superficies | interacción, presentación y confirmación | reglas de negocio |

No se crea un proveedor universal de project management. Sólo se comparten
conceptos estables: identidad, rango, duración y referencias externas.

## Descriptor estático de addon

```text
AddonDescriptor
- id y versión
- namespace de traducciones
- capacidades provistas y requeridas
- namespace de configuración
- superficies soportadas
```

Se registra manualmente bajo un Cargo feature. No hay ABI de plugins, DLLs,
descarga de código ni marketplace.

## Capacidades

Las capacidades son más precisas que `read/write` por módulo:

```text
jira.identity.read
jira.issue.read
jira.issue.comment
jira.issue.transition
jira.hours.read.self
jira.hours.write.self
time-entry.read.team
report.hours.personal
report.hours.team
bitbucket.pr.read
bitbucket.pr.review
bitbucket.pipeline.read
```

La disponibilidad efectiva es la intersección de:

```text
compilado
∩ permitido por la organización
∩ habilitado por el usuario
∩ autorizado por el proveedor
∩ válido para el ámbito
∩ expuesto en la superficie actual
```

## Capas de configuración

1. `BuiltInDefaults`: valores seguros y branding neutral.
2. `OrganizationProfile`: branding, sitios, addons permitidos, límites y políticas.
3. `UserPreferences`: conexión, ámbito, objetivos y preferencias visuales.
4. `SecretStore`: tokens por proveedor; el JSON sólo conserva referencias.
5. `SessionState`: identidad, permisos y caché; nunca es autoridad persistente.

`OrganizationProfile` se serializa como un único `organization.json` con una
sección opcional por módulo. Desktop, TUI y MCP comparten el tipo y el archivo.
`mcp.json` y `config.json` son estados locales de cada superficie y no se
distribuyen. Un JSON no instala addons: sólo configura o reduce los que el
binario ya contiene.

Un JSON editable localmente no puede conceder acceso sensible. Una política
empresarial fuerte requerirá firma o un servicio de autorización; mientras
tanto, los permisos efectivos del proveedor y el fail-closed son obligatorios.

## Compile-time y runtime

Compile-time elimina código y dependencias:

```text
hours
reports
configurable-organization
managed-distribution
mcp-management
worklogger-mcp/jira
worklogger-mcp/bitbucket
```

Runtime sólo puede reducir lo compilado. Se publicarán pocas combinaciones
probadas —Community, Managed/PM y Developer cuando corresponda— para evitar una
matriz de `2^N` variantes.

## Evolución incremental

1. Registrar los addons actuales sin cambiar comportamiento. ✓
2. Extraer la orquestación de la UI a casos de uso. En curso.
3. Agregar ports a Horas. ✓
4. Separar el transporte Jira al incorporar CRUD general. ✓ en MCP.
5. Extraer analítica y renderers reutilizables de Reportes.
6. Incorporar Bitbucket como módulo independiente. ✓ en MCP.
7. Exponer los mismos casos de uso mediante MCP, CLI o web. En curso.
