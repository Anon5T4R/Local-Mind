//! The prompt fragments, localized.
//!
//! The prose is per language; the JSON contract (`text`, `children`, `items`, and the examples) is
//! never translated - a translated key name is a broken contract. English is the source; Brazilian
//! Portuguese and Spanish have their own prose, as the original app had, and Japanese uses the
//! English prose with a localized "answer in Japanese" line (the original app had no Japanese at
//! all; a half-translated prompt would be worse than an explicit instruction).

/// One language's fragments. `{name}` placeholders are filled by [`fill`].
#[derive(Clone, Copy, Debug)]
pub struct Prompts {
    pub answer_in: &'static str,
    pub system: &'static str,
    pub chat_system: &'static str,
    pub gen_long: &'static str,
    pub gen_short: &'static str,
    pub gen_rules: &'static str,
    pub expand_ctx: &'static str,
    pub expand_avoid: &'static str,
    pub expand_body: &'static str,
    pub rephrase: &'static str,
    pub explain: &'static str,
    pub deep_avoid: &'static str,
    pub deep_body: &'static str,
}

/// The fragments for a UI language code (`pt-br`, `es`), English otherwise.
pub fn for_lang(lang: &str) -> Prompts {
    match lang {
        "pt-br" | "pt" => PT_BR,
        "es" => ES,
        "ja" => JA,
        _ => EN,
    }
}

/// A template with its `{placeholders}` filled. Unknown placeholders are left as they are, so a
/// missing value is visible instead of silently empty.
pub fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (key, value) in pairs {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

const EN: Prompts = Prompts {
    answer_in: "Answer in English.",
    system: "You are a brainstorming assistant that structures ideas into mind maps.\n{answer_in}\nAnswer ALWAYS and ONLY with valid JSON: no markdown, no comments, no text outside the JSON.\nKeep every item short (1 to 5 words when possible).",
    chat_system: "You are an assistant inside a mind map app (LocalMindmap). {answer_in} Be clear and organized, and use lists when they help. You may suggest mind map structures.",
    gen_long: "Organize the CONTENT below into a mind map, extracting the main themes and subtopics:\n\n\"\"\"{topic}\"\"\"",
    gen_short: "Create a mind map about the topic: \"{topic}\".",
    gen_rules: "Rules:\n- The central node (the root's \"text\") must be the main subject, short and specific (never \"Mind map\" or \"Topic\").\n- Generate about {breadth} first-level branches, each with a meaningful label (avoid generic ones like \"Definition\" or \"Importance\" unless they fit).\n- Depth up to {depth} levels. Every item short (1 to 5 words).\nEXACT format:\n{\"text\":\"<central subject>\",\"children\":[{\"text\":\"<branch>\",\"children\":[{\"text\":\"<subitem>\"}]}]}\nReturn only that JSON.",
    expand_ctx: "Path to this node: {path}.",
    expand_avoid: "\nDo NOT repeat or create synonyms of these items that ALREADY exist in the map: {list}.",
    expand_body: "{ctx}\nList {count} relevant and distinct subtopics specifically for \"{text}\" (and that make sense within the path above).{avoid}\nEXACT format: {\"items\":[\"...\",\"...\",\"...\"]}\nReturn only that JSON.",
    rephrase: "Rewrite the node text below following the instruction.\nText: \"{text}\"\nInstruction: {instruction}\nEXACT format: {\"text\":\"<new text>\"}\nReturn only that JSON.",
    explain: "Path: {path}.\nWrite ONE short explanation (at most one sentence, about 12 words) for \"{text}\".\nEXACT format: {\"text\":\"<explanation>\"}\nReturn only that JSON.",
    deep_avoid: "\nDo NOT repeat these items that already exist in the map: {list}.",
    deep_body: "Path to this node: {path}.\nExpand \"{text}\" into a two-level subtree: about {breadth} subtopics, each with about {sub} items.{avoid}\nShort items (1 to 5 words), specific and distinct.\nEXACT format: {\"children\":[{\"text\":\"<sub>\",\"children\":[{\"text\":\"<item>\"}]}]}\nReturn only that JSON.",
};

const PT_BR: Prompts = Prompts {
    answer_in: "Responda em português do Brasil.",
    system: "Você é um assistente de brainstorming que estrutura ideias em mapas mentais.\n{answer_in}\nResponda SEMPRE e SOMENTE com JSON válido: sem markdown, sem comentários, sem texto fora do JSON.\nMantenha cada item curto (1 a 5 palavras quando possível).",
    chat_system: "Você é um assistente dentro de um app de mapas mentais (LocalMindmap). {answer_in} Seja claro e organizado, e use listas quando ajudarem. Você pode sugerir estruturas de mapa mental.",
    gen_long: "Organize o CONTEÚDO a seguir em um mapa mental, extraindo os temas e subtemas principais:\n\n\"\"\"{topic}\"\"\"",
    gen_short: "Crie um mapa mental sobre o tema: \"{topic}\".",
    gen_rules: "Regras:\n- O nó central (o \"text\" da raiz) deve ser o assunto principal, curto e específico (NÃO use \"Mapa mental\" nem \"Tema\").\n- Gere cerca de {breadth} ramos no primeiro nível, cada um com um rótulo significativo (evite genéricos como \"Definição\" ou \"Importância\" salvo se fizer sentido).\n- Profundidade de até {depth} níveis. Cada item curto (1 a 5 palavras).\nFormato EXATO:\n{\"text\":\"<tema central>\",\"children\":[{\"text\":\"<ramo>\",\"children\":[{\"text\":\"<subitem>\"}]}]}\nRetorne apenas esse JSON.",
    expand_ctx: "Caminho até este nó: {path}.",
    expand_avoid: "\nNÃO repita nem crie sinônimos destes itens que JÁ EXISTEM no mapa: {list}.",
    expand_body: "{ctx}\nListe {count} subtópicos relevantes e distintos especificamente para \"{text}\" (e que façam sentido dentro do caminho acima).{avoid}\nFormato EXATO: {\"items\":[\"...\",\"...\",\"...\"]}\nRetorne apenas esse JSON.",
    rephrase: "Reescreva o texto do nó a seguir conforme a instrução.\nTexto: \"{text}\"\nInstrução: {instruction}\nFormato EXATO: {\"text\":\"<novo texto>\"}\nRetorne apenas esse JSON.",
    explain: "Caminho: {path}.\nEscreva UMA explicação curta (no máximo uma frase, ~12 palavras) para \"{text}\".\nFormato EXATO: {\"text\":\"<explicação>\"}\nRetorne apenas esse JSON.",
    deep_avoid: "\nNÃO repita estes itens que já existem no mapa: {list}.",
    deep_body: "Caminho até este nó: {path}.\nExpanda \"{text}\" em uma subárvore de 2 níveis: ~{breadth} subtópicos, cada um com ~{sub} itens.{avoid}\nItens curtos (1 a 5 palavras), específicos e distintos.\nFormato EXATO: {\"children\":[{\"text\":\"<sub>\",\"children\":[{\"text\":\"<item>\"}]}]}\nRetorne apenas esse JSON.",
};

const ES: Prompts = Prompts {
    answer_in: "Responde en español.",
    system: "Eres un asistente de brainstorming que estructura ideas en mapas mentales.\n{answer_in}\nResponde SIEMPRE y SOLO con JSON válido: sin markdown, sin comentarios, sin texto fuera del JSON.\nMantén cada elemento corto (1 a 5 palabras cuando sea posible).",
    chat_system: "Eres un asistente dentro de una app de mapas mentales (LocalMindmap). {answer_in} Sé claro y organizado, y usa listas cuando ayuden. Puedes sugerir estructuras de mapa mental.",
    gen_long: "Organiza el CONTENIDO de abajo en un mapa mental, extrayendo los temas y subtemas principales:\n\n\"\"\"{topic}\"\"\"",
    gen_short: "Crea un mapa mental sobre el tema: \"{topic}\".",
    gen_rules: "Reglas:\n- El nodo central (el \"text\" de la raíz) debe ser el asunto principal, corto y específico (NO uses \"Mapa mental\" ni \"Tema\").\n- Genera unos {breadth} ramos en el primer nivel, cada uno con una etiqueta significativa (evita genéricos como \"Definición\" o \"Importancia\" salvo que encajen).\n- Profundidad de hasta {depth} niveles. Cada elemento corto (1 a 5 palabras).\nFormato EXACTO:\n{\"text\":\"<tema central>\",\"children\":[{\"text\":\"<ramo>\",\"children\":[{\"text\":\"<subelemento>\"}]}]}\nDevuelve solo ese JSON.",
    expand_ctx: "Ruta hasta este nodo: {path}.",
    expand_avoid: "\nNO repitas ni crees sinónimos de estos elementos que YA EXISTEN en el mapa: {list}.",
    expand_body: "{ctx}\nEnumera {count} subtemas relevantes y distintos específicamente para \"{text}\" (y que tengan sentido dentro de la ruta de arriba).{avoid}\nFormato EXACTO: {\"items\":[\"...\",\"...\",\"...\"]}\nDevuelve solo ese JSON.",
    rephrase: "Reescribe el texto del nodo según la instrucción.\nTexto: \"{text}\"\nInstrucción: {instruction}\nFormato EXACTO: {\"text\":\"<nuevo texto>\"}\nDevuelve solo ese JSON.",
    explain: "Ruta: {path}.\nEscribe UNA explicación corta (como mucho una frase, ~12 palabras) para \"{text}\".\nFormato EXACTO: {\"text\":\"<explicación>\"}\nDevuelve solo ese JSON.",
    deep_avoid: "\nNO repitas estos elementos que ya existen en el mapa: {list}.",
    deep_body: "Ruta hasta este nodo: {path}.\nExpande \"{text}\" en un subárbol de 2 niveles: ~{breadth} subtemas, cada uno con ~{sub} elementos.{avoid}\nElementos cortos (1 a 5 palabras), específicos y distintos.\nFormato EXACTO: {\"children\":[{\"text\":\"<sub>\",\"children\":[{\"text\":\"<elemento>\"}]}]}\nDevuelve solo ese JSON.",
};

/// `ja()` cannot be a `const` because of the struct update; the match needs a value.
const JA: Prompts = Prompts {
    answer_in: "日本語で答えてください。",
    system: EN.system,
    chat_system: EN.chat_system,
    gen_long: EN.gen_long,
    gen_short: EN.gen_short,
    gen_rules: EN.gen_rules,
    expand_ctx: EN.expand_ctx,
    expand_avoid: EN.expand_avoid,
    expand_body: EN.expand_body,
    rephrase: EN.rephrase,
    explain: EN.explain,
    deep_avoid: EN.deep_avoid,
    deep_body: EN.deep_body,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Every language keeps the JSON contract byte-identical: the keys and the examples are what
    /// the parser depends on.
    #[test]
    fn the_json_contract_is_the_same_in_every_language() {
        for lang in ["en", "pt-br", "es", "ja"] {
            let prompts = for_lang(lang);
            let user = fill(prompts.expand_body, &[("ctx", "x"), ("count", "3"), ("text", "t"), ("avoid", "")]);
            assert!(user.contains("{\"items\":[\"...\",\"...\",\"...\"]}"), "{lang}: {user}");
            assert!(
                user.contains("Return only that JSON") || user.contains("Retorne apenas esse JSON") || user.contains("Devuelve solo ese JSON"),
                "{lang}"
            );
            let rules = fill(prompts.gen_rules, &[("breadth", "4"), ("depth", "2")]);
            // The keys are the contract; the example's placeholder text may be translated.
            assert!(rules.contains("\"children\":[{\"text\":\""), "{lang}: {rules}");
            assert!(rules.contains("\"text\":\""), "{lang}: {rules}");
        }
    }

    #[test]
    fn the_answer_language_follows_the_ui_language() {
        let system = |lang: &str| {
            let prompts = for_lang(lang);
            fill(prompts.system, &[("answer_in", prompts.answer_in)])
        };
        assert!(system("pt-br").contains("português"));
        assert!(system("es").contains("español"));
        assert!(system("ja").contains("日本語"));
        assert!(system("en").contains("English"));
        // An unknown code is English, not an empty prompt.
        assert_eq!(for_lang("tlh").system, EN.system);
    }

    /// Placeholders are filled; an unknown one stays visible instead of becoming empty.
    #[test]
    fn fill_replaces_what_it_knows_and_leaves_the_rest() {
        assert_eq!(fill("a {x} b {y}", &[("x", "1")]), "a 1 b {y}");
        assert_eq!(fill("no placeholders", &[("x", "1")]), "no placeholders");
    }

    #[test]
    fn the_chat_system_prompt_asks_for_the_ui_language() {
        for lang in ["en", "pt-br", "es", "ja"] {
            let chat = fill(for_lang(lang).chat_system, &[("answer_in", for_lang(lang).answer_in)]);
            assert!(chat.contains("mind map") || chat.contains("mapas mentais") || chat.contains("mapas mentales"), "{lang}");
            assert!(chat.contains(for_lang(lang).answer_in), "{lang}");
        }
    }
}
