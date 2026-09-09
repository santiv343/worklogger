# ADR 0004: perfil organizacional modular compartido

## Estado

Aceptado para implementación incremental.

## Contexto

Desktop y MCP conservan hoy configuraciones separadas. El perfil de Desktop
define branding y defaults de Jira, Horas y Reportes, mientras `mcp.json`
repite ámbitos, límites y capacidades junto con datos propios del usuario.
Esto dificulta entregar la misma experiencia a un equipo sin copiar cuentas ni
tokens.

Worklogger también permite excluir addons al compilar. Un JSON no puede afirmar
que una función está instalada ni incorporar código ausente.

## Decisión

`organization.json` será la única fuente compartible de configuración de
Worklogger. Tendrá un bloque `modules` con una sección tipada y opcional por
módulo conocido. La siguiente figura es estructura abreviada, no un perfil
ejecutable; el ejemplo completo y validable vive en
`config/example.organization.json`:

```json
{
  "schemaVersion": 2,
  "branding": {},
  "modules": {
    "jira": {
      "scopeMode": "unrestricted",
      "sites": [],
      "hours": {},
      "mcpCapabilities": []
    },
    "bitbucket": {
      "scopeMode": "unrestricted",
      "mcpCapabilities": []
    },
    "reports": {}
  }
}
```

El catálogo compilado determina qué módulos están instalados. El perfil sólo
permite y configura módulos. Las preferencias locales pueden desactivarlos o
reducir capacidades, pero nunca ampliar el perfil. Los permisos reales del
proveedor siguen siendo la autoridad final.

La disponibilidad efectiva es la intersección de:

```text
módulo compilado
∩ sección presente en organization.json
∩ preferencia local habilitada
∩ permiso de la cuenta autenticada
∩ ámbito permitido
```

El perfil no contiene correo, identidad descubierta, token ni estado de sesión.
Desktop y la TUI standalone pueden importar el mismo archivo. Ambos generan o
actualizan estado local con las selecciones personales; los secretos se guardan
exclusivamente en el almacén seguro del sistema.

`mcp.json` continúa como estado local del servidor y no se distribuye. Durante
la migración puede conservar valores resueltos para compatibilidad, pero no es
la fuente que se comparte entre usuarios.

## Semántica de módulos

- Sección ausente: la organización no configura ni ofrece ese módulo.
- Sección presente y addon compilado: el módulo puede activarse.
- Sección presente y addon no compilado: se informa como no instalado y no se
  ejecuta.
- Addon compilado y sección ausente: permanece oculto o pendiente de
  configuración según la edición.
- Un módulo desconocido para la versión instalada se rechaza; no se ejecuta ni
  se interpreta de forma parcial.

El ámbito siempre es explícito. `scopeMode: "restricted"` requiere al menos un
sitio Jira o workspace Bitbucket permitido. `scopeMode: "unrestricted"` exige
que esa lista esté vacía y declara deliberadamente que cualquier recurso
visible para la cuenta del proveedor puede seleccionarse. Un ámbito vacío no
se interpreta por inferencia.

`mcpCapabilities` y los campos `maximumAllowed*` del perfil son máximos
organizacionales; los límites restantes son defaults. El
asistente presenta ese subconjunto y cada usuario decide cuáles habilitar
localmente; nunca activa automáticamente todas las escrituras permitidas.

Jira es un módulo. Issues y Horas son grupos internos de capacidades y Horas
queda anidado en `modules.jira`. Reportes es un módulo consumidor de lecturas;
no concede permisos de Jira. Bitbucket es un módulo independiente.

## Distribuciones

Community permite importar, editar y exportar el perfil sin secretos. Managed
embebe exactamente el mismo schema tanto en Desktop como en su sidecar MCP y no
ofrece reemplazo en runtime. Los binarios personalizados se construyen desde un
perfil externo; el repositorio genérico no contiene datos de ninguna empresa.

Otros productos, como Devstation, mantienen su propio perfil y contexto. Si en
el futuro se necesita entregar varios productos con un solo archivo, se agrega
un sobre de distribución que referencie sus perfiles; no se mezclan schemas,
credenciales ni dominios dentro de Worklogger.

## Compatibilidad

El lector acepta el schema plano `1` y lo migra en memoria. Toda nueva
exportación usa schema `2`. Los errores de schema, módulo o límite fallan antes
de modificar configuración o credenciales existentes.

## Consecuencias

- Un archivo sin datos personales puede reproducir branding, módulos, ámbitos,
  límites y capacidades en Desktop y MCP.
- Agregar un módulo requiere agregar su tipo, validación, feature de build y
  adaptador de superficie; una clave JSON por sí sola no habilita código.
- La política local sigue sin ser una frontera criptográfica. Para enforcement
  centralizado futuro se necesitará un perfil firmado o un servicio de
  autorización.
