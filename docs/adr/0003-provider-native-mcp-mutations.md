# ADR 0003: mutaciones MCP con semántica de proveedor

Fecha: 2026-09-03
Estado: aceptada

## Contexto

El primer corte de Bitbucket previsto por ADR 0002 era read-only. El servidor
MCP ahora necesita cubrir también el trabajo diario sobre Jira Issues y pull
requests sin inventar una API universal ni acoplar ambos proveedores.

## Decisión

- Jira continúa como un único módulo con grupos internos de Issues y Horas.
- Bitbucket es un módulo independiente y desbundlable.
- Cada mutación conserva el modelo del proveedor y una capacidad específica.
- Ningún input elige actor; siempre se usa la cuenta autenticada.
- Toda mutación exige preview y token de confirmación de un solo uso.
- Las mutaciones sobre un PR ligan la confirmación a los commits, ramas,
  participantes y revisión exactos observados, y vuelven a compararlos
  inmediatamente antes de escribir.
- La edición de campos Jira liga la confirmación a los valores actuales de esos
  campos y los vuelve a leer antes de escribir.
- Tablero Jira y repositorios Bitbucket forman límites explícitos que fallan
  cerrados.
- Las capacidades de escritura dependen de su lectura correspondiente; no se
  puede usar una vista previa como canal lateral de lectura.

## Consecuencias

Bitbucket deja de estar limitado a lectura dentro del MCP, pero no se convierte
en dependencia de Horas ni de Jira. El Work Hub puede seguir tratándolo como
evidencia read-only. Cada distribución decide por Cargo features qué módulos
incluye y la configuración runtime sólo puede reducir capacidades.

Bitbucket Cloud y Jira Cloud no documentan una precondición de versión uniforme
para estas escrituras. Worklogger compara el snapshot confirmado justo antes de
enviar la mutación; el proveedor conserva la última validación atómica y puede
responder con conflicto. Este límite residual se acepta porque no existe una
operación compare-and-swap pública que Worklogger pueda aplicar.
