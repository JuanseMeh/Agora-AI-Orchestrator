pub struct PerformanceReportPromptBuilder;

impl PerformanceReportPromptBuilder {
    pub fn build(
        workspace_id: i32,
        assignment_id: Option<i32>,
        dataset_json: &str,
    ) -> String {
        let scope = assignment_id
            .map(|id| format!("assignment_id={}", id))
            .unwrap_or_else(|| "workspace_level".to_string());

                format!(
                        "Eres un analista académico de IA.\n\
                         Debes escribir un análisis claro, completo y accionable en español para docentes.\n\
                         Contexto:\n\
                         - workspace_id: {workspace_id}\n\
                         - scope: {scope}\n\
                         - umbral de bajo desempeño: menos de 60% del puntaje máximo\n\n\
                         Dataset (JSON):\n\
                         {dataset_json}\n\n\
                         SALIDA REQUERIDA (obligatoria):\n\
                         - Devuelve exactamente estas 3 secciones en el mismo orden, en español, cada sección con un título claro:\n\
                             1) Resumen de desempeño general\n\
                             2) Assignments con mayor índice de fallo\n\
                             3) Opciones de mejora (al menos 3 recomendaciones numeradas y accionables)\n\
                         - Usa datos numéricos del JSON cuando estén disponibles.\n\
                         - Si falta información para alguna sección, dilo explícitamente y qué información falta.\n\
                         - Termina la respuesta con la línea exacta: EXACT_END_OF_REPORT\n\n\
                         Reglas:\n\
                         - No inventes datos.\n\
                         - Máximo 300 palabras.\n\
                         - Si no puedes completar las 3 secciones por falta de información, explica por qué antes de finalizar."
                )
    }
}