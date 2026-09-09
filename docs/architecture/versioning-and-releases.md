# Versionado y releases

## Fuente de verdad

Worklogger usa Semantic Versioning. La versión canónica está en
`workspace.package.version` de `Cargo.toml`; los crates, el binario y los nombres
de instalador la heredan. El workflow copia esa versión al paquete npm antes de
publicarlo; `packages/setup/package.json` no mantiene una segunda versión
canónica. Cada versión publicada debe tener un tag `v<major>.<minor>.<patch>`
sobre el commit exacto validado.

## Qué representa una versión

- `major`: contratos o datos persistidos incompatibles.
- `minor`: capacidades compatibles nuevas.
- `patch`: correcciones compatibles sin capacidades nuevas.

Community y Managed comparten versión y código fuente. Una distribución Managed
no es un fork: es el resultado reproducible de combinar un tag de Worklogger con
un perfil externo validado. El artefacto debe registrar nombre de distribución,
versión y SHA-256 del perfil; nunca el token Jira.

## Checklist de release

1. Actualizar `Cargo.toml` y `Cargo.lock`.
2. Actualizar `CHANGELOG.md`.
3. Ejecutar formato, Clippy, tests y build para Community y Managed.
4. Commitear sin credenciales ni perfiles privados accidentales.
5. Crear el tag `v<versión>` y pushearlo.
6. Generar los instaladores y binarios MCP Windows/Linux desde ese tag y
   conservar sus SHA-256.

Un binario ya distribuido es inmutable. Cualquier rebuild con cambios requiere
una versión nueva, aunque use el mismo perfil corporativo.
