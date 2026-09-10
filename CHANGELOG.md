# Changelog

Notable Worklogger changes are documented in this file. The project uses
Semantic Versioning and dates use the `YYYY-MM-DD` format.

## [0.9.1] - 2026-09-10

### Fixed

- The TUI dashboard now shows a compact MCP-client preview and a visible link
  to the complete client list instead of silently clipping additional clients
  in a standard terminal.

## [0.9.0] - 2026-09-10

### Added

- Automatic MCP registration for Qwen Code, Gemini CLI, Kiro, and GitHub
  Copilot through their documented JSON `mcpServers` configuration.
- Automatic MCP registration for Trae Code CLI through its documented TOML
  configuration, preserving existing comments and unrelated servers.
- An MCP client compatibility guide that separates common JSON clients from
  special adapters and documents Trae IDE's project-scoped setup.

## [0.8.0] - 2026-09-09

### Added

- Desktop and MCP now use one secret-free `settings.json` document for shared
  provider preferences, scopes, limits, and the selected interface language.
- Direct hierarchical Jira and Bitbucket settings sections in the MCP TUI,
  including connection, scope, permissions, defaults, and advanced limits.
- English and Spanish interface resources. The chosen language is shared by
  Desktop and MCP and takes effect after restart.

### Changed

- Desktop and MCP preserve each other's settings and MCP consent boundaries.
- Worklogger no longer imports or modifies previous per-frontend settings
  files; a new installation starts from the canonical shared document.

## [0.7.10] - 2026-09-09

### Changed

- The complete terminal interface, confirmations, status messages, CLI help,
  and user-facing MCP errors now use English.

## [0.7.9] - 2026-09-09

### Added

- `install --config ... --clients ... --yes` installs a reviewed local MCP
  configuration without opening the TUI, optionally installs workflow skills,
  and refuses unrelated or invalid client registrations.
- A minimal Jira read-only configuration example and an assistant-specific
  installation guide for human-guided setup.

### Changed

- Public README files, user guide, npm package guide, and walkthrough diagrams
  are available in English and place MCP installation before architecture notes.
- The local-server dashboard label now describes server availability instead of
  implying that a full client installation has occurred.

## [0.7.8] - 2026-09-09

### Cambiado

- La TUI adopta una interfaz de paneles con navegación contextual, foco visible
  y una jerarquía visual uniforme en todos los flujos interactivos.
- Skills presenta una tarjeta por asistente con métricas de instalación,
  actualización y conflictos, antes y después de ejecutar el flujo.
- Los selectores, confirmaciones, campos, mensajes y estados de progreso usan
  superficies compactas que se adaptan al contenido y al tamaño de terminal.

## [0.7.7] - 2026-09-09

### Cambiado

- La TUI presenta progreso, resultados y errores dentro de la misma sesión,
  sin devolver mensajes a la consola al terminar el flujo.
- El dashboard elimina paneles anidados y usa una jerarquía visual más limpia
  para estado, acciones y foco.
- La instalación de skills muestra primero el estado por asistente, detecta
  conflictos sin sobrescribir archivos ajenos y verifica el resultado final.

## [0.7.6] - 2026-09-09

### Cambiado

- La TUI mantiene una única sesión visual al navegar entre sus pantallas; volver
  con `Esc` ya no expone la consola entre pasos.
- La configuración de Bitbucket puede conservar reviewers adicionales y la
  preferencia de cerrar la rama fuente para los PRs futuros.

### Corregido

- Un merge asíncrono informa que sigue pendiente e incluye el identificador de
  tarea en lugar de reportarse como terminado.
- Desktop conserva la hora, zona horaria y segundos al editar una carga sin
  cambiar esos campos.
- Se corrigieron la espera de procesos de clientes, la instalación parcial de
  skills, el roster de reportes y la propagación de errores de Clippy en Windows.

## [0.7.5] - 2026-09-09

### Cambiado

- La configuración presenta Jira y Bitbucket como tarjetas descriptivas y
  agrupa las capacidades de cada integración en una única selección.
- Los campos de texto ahora permiten editar valores predeterminados, mover el
  cursor y pegar contenido; los secretos permanecen enmascarados.
- Las listas usan una jerarquía visual consistente, color de foco y navegación
  por teclado o mouse.

### Corregido

- `Esc` y `q` cancelan el paso actual sin elegir accidentalmente otra opción;
  al volver desde el menú principal se conserva el dashboard.
- Los clics fuera de una lista y los clics sobre listas desplazadas ya no
  seleccionan elementos incorrectos.

## [0.7.4] - 2026-09-09

### Cambiado

- Todo el asistente MCP usa la TUI: proveedores, sitios, tableros,
  repositorios, capacidades, clientes, confirmaciones y campos de texto.
- Los tokens se ingresan en un campo enmascarado dentro de la TUI.

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
