# Agora AI Orchestrator

Este repositorio contiene el servicio `Agora-AI-Orchestrator`, un orquestador gRPC para tareas de calificación asistida por IA, generación de sugerencias y reportes de desempeño.

## Resumen

El servicio expone un API gRPC que coordina:
- la extracción de contexto de tareas y entregas,
- la evaluación con un modelo LLM (Gemini),
- la persistencia de calificaciones en un servicio de workspace,
- la generación de reportes de desempeño con análisis AI.

No expone una API HTTP REST directamente; la interacción se hace a través de gRPC.

## Componentes principales

### `src/main.rs`
- Inicializa el servicio y los clientes necesarios.
- Lee variables de entorno para configurar:
  - `GRPC_PORT` (default `50051`),
  - `GEMINI_API_KEY`,
  - `WORKSPACE_SERVICE_URL`,
  - `USER_SERVICE_URL`,
  - `SUGGESTION_TTL_HOURS`.
- Crea instancias compartidas de:
  - `GeminiClient` (proveedor LLM),
  - `Orchestrator`,
  - `ContextAggregator`,
  - `WorkspaceClient`,
  - `UserConfigClient`,
  - `SuggestionCache`.
- Lanza el servidor gRPC.

### `src/api/server.rs`
- Construye el handler `GradingHandler`.
- Registra el servicio gRPC `AiServiceServer`.
- Arranca el servidor en el puerto definido.

### `src/api/handlers/grading_handler.rs`
- Implementa los métodos gRPC del servicio:
  - `SuggestAssignment`
  - `ApproveSuggestion`
  - `GradeAssignment`
  - `GeneratePerformanceReport`
- Valida entradas, arma filtros y contexto,
- Invoca el orquestador y persiste datos cuando corresponde.

### `src/orchestration/orchestrator.rs`
- El orquestador centraliza la lógica de despacho.
- Recibe un `OrchestratorRequest` y enruta a un workflow.
- Hoy soporta `GradingWorkflow`.

### `src/orchestration/intent_router.rs`
- Mapea `OrchestratorRequest` a `WorkflowType`.
- Es la capa de decisión que separa la API de los workflows.

### `src/application/prompts/performance_report_prompt.rs`
- Construye el prompt que se envía al modelo LLM para generar el texto de análisis.
- Define el formato esperado de la respuesta.

### `src/integration/gemini/client.rs`
- Implementa `LlmProvider` para Gemini.
- Envía prompts de texto y recibe respuestas del modelo.

### `src/context/aggregator.rs`
- Construye el contexto de calificación desde el servicio de workspace.
- Aplica filtros de submissions y users.

### `src/context/workspace_client.rs`
- Interactúa con el servicio de workspace para leer datos de entregas y escribir calificaciones.

### `src/context/user_config_client.rs`
- Consulta el perfil AI del usuario.
- Valida `agentic_mode` para permitir sugerencias.

### `src/context/suggestion_cache.rs`
- Guarda sugerencias de calificación temporales.
- Permite aprobar sugerencias posteriormente.

## API gRPC expuesta

### Servicio `AiService`

#### `SuggestAssignment`
- Request:
  - `workspace_id`: int32
  - `assignment_id`: int32
  - `requester_user_id`: string (UUID)
  - `submission_ids`: repeated int32
  - `user_ids`: repeated string
  - `include_already_graded`: bool
- Response:
  - `suggestion_id`: string
  - `results`: repeated `GradingResult`
  - `stats`: `SuggestionStats`

##### Propósito
Generar una propuesta de calificaciones sin persistirlas.

##### Flujo
1. Valida `requester_user_id`.
2. Comprueba `agentic_mode` del usuario.
3. Crea contexto de calificación filtrado.
4. Llama a `Orchestrator.dispatch(...)` con `GradeAssignment`.
5. Guarda la sugerencia en cache con `suggestion_id`.
6. Devuelve resultados y estadísticas.

#### `ApproveSuggestion`
- Request:
  - `suggestion_id`: string
- Response:
  - `suggestion_id`: string
  - `results`: repeated `GradingResult`

##### Propósito
Convertir una sugerencia previamente generada en calificaciones definitivas.

##### Flujo
1. Recupera la sugerencia del cache.
2. Elimina la entrada del cache.
3. Persiste las calificaciones con `WorkspaceClient::write_grade`.
4. Retorna los resultados.

#### `GradeAssignment`
- Request:
  - `workspace_id`: int32
  - `assignment_id`: int32
  - `submission_ids`: repeated int32
  - `user_ids`: repeated string
  - `include_already_graded`: bool
- Response:
  - `results`: repeated `GradingResult`

##### Propósito
Calificar y persistir directamente sin pasar por la fase de sugerencia.

##### Flujo
1. Arma contexto de calificación filtrado.
2. Ejecuta el flujo de calificación vía `Orchestrator`.
3. Persiste cada resultado en el workspace.
4. Devuelve las calificaciones.

#### `GeneratePerformanceReport`
- Request:
  - `workspace_id`: int32
  - `assignment_id`: optional int32
- Response:
  - `workspace_id`: int32
  - `assignment_id`: optional int32
  - `total_assignments`: int32
  - `total_submissions`: int32
  - `graded_submissions`: int32
  - `pending_submissions`: int32
  - `average_score`: double
  - `ai_analysis`: string
  - `assignments`: repeated `AssignmentPerformance`
  - `students`: repeated `StudentPerformance`

##### Propósito
Generar un reporte de desempeño cuantitativo acompañado de un análisis generado por IA.

##### Flujo
1. Obtiene datos de desempeño con `WorkspaceClient::fetch_performance_data(...)`.
2. Convierte esos datos a JSON.
3. Genera un prompt con `PerformanceReportPromptBuilder`.
4. Llama a `provider.generate_text(prompt)`.
5. Retorna métricas y el texto de análisis.

## Objetos clave

### `GradingResult`
- `result_id`: UUID string
- `submission_id`: int32
- `total_score`: double
- `max_score`: double
- `feedback_summary`: string
- `grading_model`: string
- `evaluated_at`: timestamp string
- `criteria_results`: repeated `CriterionResult`

### `CriterionResult`
- `criterion_id`: string
- `criterion_name`: string
- `score`: double
- `max_score`: double
- `feedback`: string
- `matched_level`: string

### `SuggestionStats`
- `average_score`: double
- `max_score`: double
- `graded_submissions`: int32

### `AssignmentPerformance`
- `assignment_id`: int32
- `assignment_name`: string
- `total_submissions`: int32
- `graded_submissions`: int32
- `pending_submissions`: int32
- `average_score`: double
- `max_score`: double
- `failed_submissions`: int32
- `failure_rate`: double

### `StudentPerformance`
- `user_id`: string
- `total_submissions`: int32
- `graded_submissions`: int32
- `pending_submissions`: int32
- `average_score`: double

## Cómo ejecutar

1. Configura las variables de entorno necesarias:
```bash
export GRPC_PORT=50051
export GEMINI_API_KEY=...
export WORKSPACE_SERVICE_URL=...
export USER_SERVICE_URL=...
export SUGGESTION_TTL_HOURS=24
```

2. Ejecuta el servicio:
```bash
cargo run
```

3. Llama a los métodos gRPC con `grpcurl`.

## Ejemplos `grpcurl`

### `SuggestAssignment`
```bash
grpcurl -plaintext -import-path proto -proto ai_service.proto \
  -d '{"workspace_id":1,"assignment_id":42,"requester_user_id":"00000000-0000-0000-0000-000000000000","submission_ids":[101],"user_ids":["user-1"],"include_already_graded":false}' \
  localhost:50051 ai.AiService/SuggestAssignment
```

### `ApproveSuggestion`
```bash
grpcurl -plaintext -import-path proto -proto ai_service.proto \
  -d '{"suggestion_id":"11111111-2222-3333-4444-555555555555"}' \
  localhost:50051 ai.AiService/ApproveSuggestion
```

### `GradeAssignment`
```bash
grpcurl -plaintext -import-path proto -proto ai_service.proto \
  -d '{"workspace_id":1,"assignment_id":42,"submission_ids":[101],"user_ids":["user-1"],"include_already_graded":false}' \
  localhost:50051 ai.AiService/GradeAssignment
```

### `GeneratePerformanceReport`
```bash
grpcurl -plaintext -import-path proto -proto ai_service.proto \
  -d '{"workspace_id":1,"assignment_id":42}' \
  localhost:50051 ai.AiService/GeneratePerformanceReport
```

## Notas importantes
- El servicio es principalmente un orquestador: no implementa la lógica de negocio de workspace ni la UI.
- `SuggestAssignment` es una operación no destructiva; `ApproveSuggestion` persiste la calificación.
- `GradeAssignment` persiste de inmediato.
- `GeneratePerformanceReport` depende del modelo LLM para producir `ai_analysis`.
- Si se necesita soporte REST, hay que agregar un gateway HTTP por encima del gRPC.

grpcurl -plaintext -import-path /home/javizzz/Documents/Agora/Test_services/Agora-AI-Orchestrator/proto   -proto ai_service.proto -d '{}
ai.AiService/ENDPOINT
