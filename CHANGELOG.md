# Changelog

Todos los cambios relevantes de Worklogger se documentan en este archivo. El
proyecto usa Semantic Versioning y las fechas se expresan como `AAAA-MM-DD`.

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
