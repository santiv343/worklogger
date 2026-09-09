# Permisos y límites de confianza

## Principios obligatorios

1. La identidad se obtiene siempre del proveedor autenticado.
2. El autor de un worklog nunca es un input del usuario.
3. Crear horas utiliza la cuenta autenticada.
4. Editar o eliminar vuelve a leer el worklog y verifica ownership.
5. Horas de terceros son exclusivamente de lectura cuando Jira las permite.
6. Toda mutación muestra destino, identidad, valores e impacto antes de confirmar.
7. Cancelar es la acción segura predeterminada.
8. Los tokens no entran en JSON, logs, reportes ni mensajes de error.

## Fuentes de autorización

Un control está disponible sólo si coinciden:

- capacidad incluida en el binario;
- política de la distribución;
- preferencia del usuario;
- permiso efectivo devuelto por Jira o Bitbucket;
- ownership y ámbito del recurso.

El perfil organizacional no eleva permisos. Incluso una edición Managed sólo
restringe configuración; Jira y Bitbucket siguen siendo la autoridad.

## Reportes de equipo

- son siempre read-only;
- deben incluir personas del ámbito aunque no tengan horas;
- sólo muestran información visible para la cuenta autenticada;
- el default genérico exige administración del proyecto;
- una política local menos restrictiva no reemplaza autorización empresarial.

Para controles centralizados habrá que incorporar una política firmada o un
servicio organizacional. No se simula ese control con un flag editable.

## Datos derivados

- Cada registro mantiene enlace al dato fuente.
- Las inferencias issue–PR indican el criterio usado.
- Los cachés se segmentan por conexión, identidad, ámbito y fecha.
- Datos parciales se muestran con advertencia; no como totales confiables.
- Ninguna métrica debe convertirse en puntuación de rendimiento individual.

## Distribuciones Managed

El perfil se incorpora al compilar y no puede reemplazarse en runtime. El
secreto de cada usuario continúa fuera del perfil: Windows usa Credential
Manager y Linux un almacén atómico privado con directorio `0700` y archivos
`0600`. La edición corporativa no debe incluir tokens del equipo de build.

## Registro en clientes MCP

- Se detectan clientes antes de ofrecer una acción; no se crean carpetas para
  aplicaciones ausentes.
- Desktop y TUI muestran el destino exacto y exigen confirmación antes de
  instalar o quitar.
- Sólo se crea, actualiza o elimina la entrada `worklogger`; las demás
  integraciones se preservan.
- El retiro exige el argumento `serve` y una ruta exacta o una versión anterior
  dentro del directorio MCP propio; una entrada homónima distinta se trata como
  conflicto y nunca se borra.
- Las escrituras JSON son atómicas, tienen límite de tamaño y fallan cerradas si
  el documento existente es inválido, cambia durante la operación o es un
  enlace simbólico.
- El registro contiene únicamente la ruta absoluta del servidor y el argumento
  `serve`; nunca copia tokens ni credenciales.
- Desactivar una capacidad reduce las tools expuestas aunque un cliente conserve
  registrado el servidor.
- Desktop y MCP usan namespaces de credenciales distintos. Activar MCP copia el
  token al namespace MCP; desinstalarlo nunca elimina la credencial de Desktop.
- Desconexión y desinstalación serializan configuración y credenciales entre
  procesos; si falla un borrado, restauran el snapshot anterior y no anuncian
  éxito.
- Linux rechaza directorios y archivos de credenciales enlazados o accesibles
  por grupo/otros, y usa un bloqueo no bloqueante para evitar dos cambios
  simultáneos.
- El servidor se copia a una ruta estable y versionada del usuario antes de
  registrar un cliente. No se registra una ruta dentro de un portable movible.
