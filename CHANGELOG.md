# Changelog

Todos los cambios relevantes de Worklogger se documentan en este archivo. El
proyecto usa Semantic Versioning y las fechas se expresan como `AAAA-MM-DD`.

## [0.7.3] - 2026-09-09

### Cambiado

- El menú interactivo ahora usa una TUI con paneles, colores, navegación por
  teclado y selección mediante mouse.

## [0.7.2] - 2026-09-09

### Corregido

- La publicación npm configura explícitamente la autenticación del registro
  público mediante el secreto de CI.

## [0.7.1] - 2026-09-09

### Corregido

- El paquete npm se valida y publica independientemente del instalador desktop.
- El fixture de estado de Codex es portable entre shells Unix de CI.

## [0.7.0] - 2026-09-09

### Agregado

- Bootstrap MCP público y skills de flujo instalables sin una cuenta o token de
  Worklogger.
- Licencia MIT para el código y el paquete npm.

### Cambiado

- El pipeline público distribuye solamente la edición Community. Los perfiles
  Managed se construyen por separado, con configuración externa.
