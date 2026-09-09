# Reglas de trabajo de Worklogger

Este archivo complementa las reglas del repositorio padre. Si existe un
conflicto, se aplica la regla más restrictiva.

## Continuidad obligatoria

1. Leer `status.md` antes de explorar o modificar código.
2. Antes de implementar, actualizar su checklist y dejar un único ítem marcado
   como `[-]` en la sección **En curso**.
3. Actualizar `status.md` después de cada decisión, cambio funcional, validación,
   bloqueo o modificación relevante del alcance.
4. Antes de finalizar, registrar qué quedó terminado, la evidencia ejecutada y
   la próxima acción concreta.
5. Nunca marcar una tarea como completa si sus checks requeridos no pasaron.

`status.md` es el estado operativo para retomar trabajo. Los ADR explican las
decisiones duraderas y los documentos de producto conservan el roadmap amplio;
no se debe duplicar todo su contenido en el status.

## Comunicación sin ambigüedades

- Escribir en español y comenzar por el resultado o estado real.
- Distinguir siempre entre `planificado`, `en implementación`, `implementado`,
  `probado` y `bloqueado`.
- No usar “listo”, “funciona” o “terminado” si falta algún check requerido.
- Para trabajo parcial, indicar exactamente qué flujo ya funciona, cuál no y
  cuál es la próxima acción.
- Toda decisión debe explicitar alcance, consecuencia y elementos postergados.
- No ocultar warnings, fallos, supuestos ni limitaciones detrás de un resumen.
- Durante trabajo prolongado, informar cada avance material sin dejar al usuario
  sin contexto de qué se está ejecutando.

## Preferencias de producto y colaboración

- Priorizar utilidad demostrable del MVP y una UX clara, moderna e iterable.
- Mantener una forma directa de ejecutar y ver cambios durante el desarrollo;
  evitar reinstalaciones para cada iteración.
- La aplicación canónica es genérica: ningún nombre, URL, tablero, campo, color,
  texto o regla de una empresa puede quedar hardcodeado.
- Las personalizaciones corporativas entran mediante perfiles externos de build.
- Las ediciones Managed excluyen el cambio de configuración organizacional; las
  ediciones Community permiten configuración manual y JSON.
- Las capacidades son modulares, opcionales y desbundlables cuando sus
  dependencias lo permitan.
- Desktop es la superficie principal actual; MCP, CLI y web son adaptadores
  opcionales futuros, no implementaciones paralelas del negocio.
- La identidad siempre proviene de la cuenta autenticada. Horas de terceros son
  sólo lectura cuando el proveedor lo autoriza; nunca se modifican.
- Toda mutación sensible muestra identidad, destino, efecto y confirmación.
- Los secretos viven en el almacén seguro del sistema y nunca en JSON, código,
  documentación, logs, reportes o mensajes al usuario.
- No calcular ni cargar horas automáticamente desde commits, PRs o reuniones;
  sólo presentarlos como evidencia para una decisión humana.
- No crear abstracciones, dependencias o servicios “por si acaso”. Implementar
  el corte mínimo robusto y validar con datos reales antes de generalizar.
- Para decisiones arquitectónicas, seguridad o releases importantes, solicitar
  revisiones independientes y registrar tanto objeciones como conclusiones.
- Preservar cambios ajenos y no mezclar correcciones fuera de alcance.
- Favorecer calidad, claridad y evidencia por encima de ahorrar tokens o dejar
  una implementación a medias.
- Si el usuario pide continuar, ejecutar la próxima acción registrada; no
  reemplazar trabajo concreto por otra explicación del plan.

## Preferencias de UX y distribución

- La navegación principal representa módulos (`Jira`, `Reportes`, futuros
  addons); Horas pertenece a Jira y no se presenta como módulo independiente.
- Configuración se abre como modal global, con navegación lateral para General
  y cada módulo instalado.
- Una actualización parcial usa skeletons sólo sobre los datos afectados; no
  bloquea ni reemplaza toda la pantalla.
- Acciones compactas usan iconos reconocibles y tooltip. Acciones cuyo propósito
  no es evidente, como exportar, conservan una etiqueta visible.
- Selectores de tablero, tarea y fechas son buscables cuando el volumen lo
  requiere, cierran al hacer click afuera y nunca aceptan fechas futuras.
- Los presets de fecha tienen semántica estable; las flechas navegan con la misma
  unidad del preset seleccionado y el texto describe el período real.
- El onboarding pide sólo datos disponibles en ese momento: valida credenciales,
  descubre recursos y recién después permite seleccionar el ámbito.
- Reportes personales y de equipo son vistas separadas. La vista de equipo sólo
  aparece con permiso/configuración explícitos y nunca habilita mutaciones de
  horas ajenas.
- Los filtros de equipo se construyen desde el ámbito y sus colaboradores, no
  sólo desde quienes ya cargaron horas; cero horas también es información.
- Una exportación representa la vista completa y agrega datos crudos trazables;
  un formato no implementado o deshabilitado no se presenta como disponible.
- La edición para usuarios finales debe ejecutarse sin Node, Rust ni comandos.
  El desarrollo debe conservar hot reload y una iteración directa.
- Las pruebas con cuentas reales son read-only salvo orden expresa de mutar; los
  tests automáticos usan dobles o servidores aislados.

## Formato del status

- `[ ]`: pendiente.
- `[-]`: única tarea activa.
- `[x]`: terminada con evidencia.
- `[!]`: bloqueada, incluyendo causa y condición para desbloquear.

Mantener siempre actualizados: fecha UTC, rama, objetivo actual, checklist,
decisiones recientes, riesgos, validaciones y próxima acción. No guardar tokens,
credenciales, correos personales ni datos privados.

## Arquitectura

- Los puertos pertenecen al caso de uso consumidor.
- El dominio no importa UI, HTTP ni adaptadores de proveedores.
- Normalizar sólo conceptos comprobados por al menos un caso de uso real.
- Conservar identidad, permisos y referencias dentro del namespace de su
  conexión.
- Una capacidad mostrada por la UI nunca sustituye autorización ni ownership en
  la operación real.
- Los errores parciales conservan origen y trazabilidad.
- Evitar migraciones horizontales masivas; preferir cortes verticales probados.

## Implementación

- Empezar con un test que falle para comportamiento nuevo o refactors de riesgo.
- Mantener una sola fuente de verdad para cada concepto compartido: IDs y nombres
  de tools salen de su catálogo; módulos y permisos de sus enums; defaults y
  límites de configuración tipada; textos visibles de los recursos de idioma.
  Consumidores, UI y tests deben derivarlos de esa fuente en vez de repetir
  literales. No extraer literales incidentales sin semántica compartida.
- Mantener funciones nuevas por debajo de 20 líneas.
- No crear interfaces especulativas ni dependencias sin un uso actual.
- Actualizar documentación y `status.md` en el mismo cambio que una frontera
  arquitectónica.
- Ejecutar formato, Clippy, tests y checks de features afectados antes de cerrar.
