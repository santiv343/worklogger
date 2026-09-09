# Herramientas MCP de proveedores

## Alcance

Worklogger expone operaciones de trabajo diario mediante addons compilables de
Jira Cloud y Bitbucket Cloud. Un addon conoce el protocolo de su proveedor, no
la estructura de una empresa. Sitios, ámbitos, proyectos, repositorios, ramas,
tipos de issue, estados, campos y reviewers son datos descubiertos o
configurados.

Jira es un único módulo. Issues y Horas son grupos de capacidades internos que
comparten conexión, identidad y tablero. Bitbucket es otro módulo y no depende
de Jira.

No se crea una API universal de tickets o pull requests. Las superficies pueden
componer información, pero cada mutación conserva la semántica y los permisos
del proveedor que la ejecuta.

## Inventario del legacy

| Proveedor | Se conserva | Se reemplaza | Se descarta |
|---|---|---|---|
| Jira | detalle, JQL, transiciones, comentario | respuestas tipadas, campos editables descubiertos, confirmación y capacidades separadas | IDs de custom fields y nombres de estados fijos |
| Bitbucket | detalle, listado, actividad, reviews, creación, comentario, aprobación y cierre | edición real de PR, ámbitos configurados y token moderno | ramas, repos, reviewers y notificaciones propios de una empresa |

## Catálogo objetivo

### Jira

| Tool | Capacidad | Efecto |
|---|---|---|
| `jira_get_issue` | `jira.issue.read` | detalle visible del issue |
| `jira_search_issues` | `jira.issue.read` | búsqueda JQL acotada con indicador `hasMore` |
| `jira_get_edit_metadata` | `jira.issue.read` | campos que Jira permite editar |
| `jira_get_transitions` | `jira.issue.read` | transiciones actualmente disponibles |
| `jira_update_issue` | `jira.issue.edit` | modifica los campos confirmados |
| `jira_add_comment` | `jira.issue.comment` | agrega un comentario |
| `jira_transition_issue` | `jira.issue.transition` | aplica una transición por ID |
| `jira_get_my_hours` | `jira.hours.read.self` | consulta horas propias |
| `jira_get_my_unlogged_issues` | `jira.hours.read.self` | tareas propias del sprint sin worklogs propios en el período |
| `jira_create_worklog` | `jira.hours.write.self` | crea una carga de la cuenta autenticada |

La edición recibe un mapa de campos porque Jira define campos estándar y
custom por proyecto. El agente debe consultar primero los metadatos editables;
Worklogger no inventa IDs ni traduce nombres de estados. Las transiciones se
ejecutan por ID para evitar coincidencias ambiguas por texto.

Crear una carga exige un instante RFC 3339 con el desfase horario configurado y
una duración entera en minutos. El input nunca contiene autor. La vista previa
incluye coincidencias propias del mismo issue, fecha y duración; el agente debe
mostrarlas antes de solicitar confirmación. La ejecución revalida el tablero y
la lista de coincidencias.

### Bitbucket Cloud

| Tool | Capacidad | Efecto |
|---|---|---|
| `bitbucket_list_repositories` | `bitbucket.pr.read` | repos visibles dentro de un workspace permitido |
| `bitbucket_list_pull_requests` | `bitbucket.pr.read` | PRs filtrados de un repositorio permitido |
| `bitbucket_get_pull_request` | `bitbucket.pr.read` | detalle, participantes y links |
| `bitbucket_get_pull_request_activity` | `bitbucket.pr.read` | actividad reciente |
| `bitbucket_create_pull_request` | `bitbucket.pr.create` | crea un PR con origen y destino explícitos |
| `bitbucket_update_pull_request` | `bitbucket.pr.edit` | edita título, descripción, destino o estado de cierre de rama |
| `bitbucket_add_pull_request_comment` | `bitbucket.pr.comment` | agrega un comentario |
| `bitbucket_approve_pull_request` | `bitbucket.pr.review` | aprueba como la cuenta autenticada |
| `bitbucket_unapprove_pull_request` | `bitbucket.pr.review` | retira la aprobación propia |
| `bitbucket_request_changes` | `bitbucket.pr.review` | solicita cambios como la cuenta autenticada |
| `bitbucket_remove_change_request` | `bitbucket.pr.review` | retira la solicitud propia |
| `bitbucket_merge_pull_request` | `bitbucket.pr.merge` | fusiona el PR con estrategia explícita |
| `bitbucket_decline_pull_request` | `bitbucket.pr.decline` | cierra el PR sin fusionarlo |

Merge y decline son capacidades independientes. Habilitar edición o review no
las habilita implícitamente.

Al crear un pull request, Worklogger consulta los *effective default reviewers*
de Bitbucket: los definidos en el repositorio y los heredados del proyecto. Los
combina sin duplicados con los reviewers indicados en el pedido y muestra la
lista resuelta en la vista previa. `closeSourceBranch` también se muestra allí;
si se omite, conserva `false` y la rama fuente no se elimina.

La TUI MCP distribuye las instrucciones de flujo como tres carpetas `SKILL.md`
portables, bajo nombres `worklogger-*`. Las instala en los destinos globales de
Agent Skills, Codex, Claude Code y Windsurf, con una marca de ownership para no
sobreescribir una skill homónima ajena. Cursor mantiene un modelo distinto de
Rules y Commands y no recibe una pseudo-skill incompatible.

## Autorización y confirmación

La disponibilidad efectiva de una tool es la intersección ya definida por la
arquitectura modular. Además:

1. ningún input acepta una identidad con la cual ejecutar;
2. el adaptador obtiene la identidad autenticada del proveedor;
3. toda escritura comienza con una vista previa de identidad, destino y efecto;
   además de `preview` estructurado, la respuesta devuelve `visiblePreview`
   para que el cliente la pegue como texto visible antes de solicitar confirmación;
4. la ejecución exige `confirmed: true` y el token de un solo uso de esa vista;
5. toda capacidad de escritura exige también la capacidad de lectura del mismo
   recurso, para impedir que una vista previa eluda el permiso de lectura;
6. el token queda ligado al pedido y al estado remoto incluido en la vista
   previa; al confirmar, ese contexto se consulta nuevamente y el token no se
   reutiliza;
7. el issue debe pertenecer al tablero Jira y el PR al repositorio permitido;
8. Jira y Bitbucket son la autoridad final y un `403` falla cerrado;
9. ningún token de proveedor entra al JSON MCP, argumentos, logs o respuestas.

En Bitbucket, la revisión confirmada incluye título, descripción, ramas, hashes
de origen y destino, participantes, estado y `updated_on`. Toda mutación de un
PR vuelve a comparar ese snapshot inmediatamente antes de enviarla. La edición
Jira relee los campos afectados y exige que sigan iguales. Los proveedores no
documentan una precondición de versión uniforme para estas operaciones; su
respuesta de conflicto sigue siendo la autoridad atómica final.

Las annotations MCP complementan el contrato: las lecturas son `readOnly` y
todas las escrituras se anuncian como `destructive` para que el cliente nunca
las ejecute como si fueran observaciones inocuas.

## Configuración

Los secretos viven en el almacén seguro por proveedor y propósito. El JSON sólo
conserva:

- conexión Jira compartida: origen Cloud, correo, tablero y límites, con la
  configuración opcional de Horas anidada dentro del módulo;
- conexión Bitbucket Cloud: correo, workspaces permitidos y límites;
- módulos y capacidades habilitadas.

Las escrituras coordinadas de configuración y credenciales usan un bloqueo
exclusivo por usuario en Windows, además de rollback compensatorio, para que la
TUI y Desktop no puedan intercalar una actualización parcial.

El endpoint oficial de cada adaptador es una constante del protocolo, no una
personalización empresarial. No se permite reemplazarlo por una URL arbitraria
que pueda recibir credenciales. Jira Data Center, Bitbucket Data Center y otros
proveedores requieren adaptadores separados.

## Orden de entrega

1. Contrato de configuración y catálogo con tests.
2. Jira operativo y probado contra HTTP aislado.
3. Bitbucket Cloud operativo y probado contra HTTP aislado.
4. Selección de proveedor y capacidades desde la TUI; gestión de capacidades y
   clientes ya configurados desde Desktop.
5. Matriz de tests MCP aislados, builds desbundlados y prueba read-only opcional
   contra cuentas reales.
