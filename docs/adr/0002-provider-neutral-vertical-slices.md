# ADR 0002: neutralidad por cortes verticales

Fecha: 2026-09-03
Estado: aceptada; alcance Bitbucket actualizado por ADR 0003

## Contexto

Worklogger debe poder incorporar Jira, Notion, proveedores SCM y sistemas
propios. Sin embargo, el dominio actual de Horas, la sesión y los reportes aún
usan conceptos y DTOs Jira. Una normalización horizontal inmediata tendría un
radio de cambio alto y asumiría equivalencias que todavía no fueron comprobadas.

Dos revisiones independientes coincidieron en que Jira + Bitbucket valida la
composición de roles diferentes, pero no demuestra que Jira y Notion sean
intercambiables como fuentes de tareas o destinos de horas.

## Decisión

- Mantener la neutralidad de proveedor como regla de dependencias.
- Crear puertos estrechos propiedad de cada caso de uso.
- Migrar un flujo vertical por vez, comenzando por horas personales.
- Mantener Jira como primer origen y destino de horas.
- Incorporar Bitbucket primero como evidencia opcional y read-only. Las
  mutaciones provider-native posteriores se rigen por ADR 0003.
- Confirmar una abstracción con una segunda implementación real antes de
  declararla estable o pública.

Los primeros puertos previstos son lectura de horas propias, mutaciones propias,
lectura de tareas candidatas y, cuando exista Bitbucket, lectura de evidencia.
Identidad y capacidades pertenecen a la conexión autenticada; no son servicios
universales independientes.

## Límites

- No existe un modelo universal de project management.
- Estados, transiciones, comentarios enriquecidos, PRs y pipelines conservan su
  semántica de proveedor.
- Las referencias incluyen conexión e identificador opaco.
- Un destino de horas sólo acepta referencias compatibles.
- Una capacidad cacheada puede orientar la UI, pero nunca autorizar una
  mutación.
- No se crea almacenamiento propio, marketplace ni plugin ABI en esta etapa.

## Consecuencias

La migración será incremental y mantendrá funcionando la aplicación. Durante un
tiempo coexistirán modelos personales neutrales y reportes de equipo específicos
de Jira. Esa duplicación transitoria es preferible a un cambio masivo que
arriesgue ownership, exportaciones y permisos.

Mientras la configuración admita una sola conexión por sitio, el primer
adaptador usa el origen del sitio Jira como namespace estable. Antes de admitir
dos conexiones al mismo sitio será obligatorio persistir un identificador de
conexión propio y migrar esas referencias; no se concatenarán correo ni token.

Notion se evaluará inicialmente como fuente read-only con mapeo explícito de
propiedades sólo cuando exista demanda. Un almacenamiento propio de horas exige
resolver autenticación, sincronización, auditoría, backup y retención, por lo que
no forma parte del siguiente MVP.

## Validación

El diseño se considerará probado cuando:

1. el flujo personal funcione contra un puerto y un adaptador en memoria;
2. Jira implemente ese puerto sin perder errores parciales ni ownership;
3. Bitbucket pueda enriquecer el asistente sin ser una dependencia obligatoria;
4. una segunda fuente real fuerce o confirme los contratos antes de publicarlos.
