# ADR 0001: una base de código para múltiples distribuciones

Fecha: 2026-09-03
Estado: aceptada

## Contexto

Se necesita una edición genérica configurable y ediciones preparadas para
organizaciones cuyo perfil no pueda modificarse. Mantener un repositorio o fork
por empresa duplicaría correcciones, seguridad, tests y releases.

## Decisión

Mantener una única base de código genérica.

- Community compila `configurable-organization`.
- Managed compila `managed-distribution` con un perfil externo indicado por
  `WORKLOGGER_DISTRIBUTION_PROFILE`.
- El build copia el perfil validado al artefacto generado de Cargo.
- Managed no compila la UI de importación/exportación y no consulta overrides.
- Perfiles organizacionales reales viven fuera del repositorio genérico.

## Consecuencias positivas

- cada corrección llega a todas las organizaciones;
- no hay datos corporativos en el producto base;
- los instaladores son reproducibles;
- el bloqueo no depende de ocultar botones;
- futuras ediciones pueden elegir addons mediante Cargo features.

## Costos y límites

- cada combinación publicada debe probarse explícitamente;
- el branding del instalador sigue siendo común hasta incorporar generación de
  metadata e iconos por distribución;
- un binario Managed restringe configuración, pero no reemplaza permisos del
  proveedor ni una política central firmada;
- la firma de código continúa siendo un proceso externo.

## Alternativas descartadas

- repositorio o fork por empresa;
- distribuir un JSON editable junto al instalador;
- descargar perfiles desde un servicio antes de validar valor de producto;
- sistema dinámico de plugins o marketplace.
