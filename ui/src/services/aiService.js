
const GEMINI_API_URL = 'https://generativelanguage.googleapis.com/v1beta/models/gemini-pro:generateContent';

export const generateSQL = async (prompt, schemaContext, apiKey) => {
    if (!apiKey) throw new Error("API Key is required");

    const fullPrompt = `
You are an expert SQL assistant. Convert the following natural language request into a valid SQL query.
Schema Context:
${JSON.stringify(schemaContext, null, 2)}

Request: "${prompt}"

Return ONLY the raw SQL query. Do not use markdown formatting (like \`\`\`sql). Do not include explanations.
    `.trim();

    try {
        const response = await fetch(`${GEMINI_API_URL}?key=${apiKey}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                contents: [{ parts: [{ text: fullPrompt }] }]
            })
        });

        const data = await response.json();
        if (!response.ok) {
            throw new Error(data.error?.message || "Failed to generate SQL");
        }

        let sql = data.candidates?.[0]?.content?.parts?.[0]?.text || "";
        // Cleanup markdown if existing
        sql = sql.replace(/```sql/g, '').replace(/```/g, '').trim();
        return sql;

    } catch (error) {
        console.error("Gemini API Error:", error);
        throw error;
    }
};

export const generateInsights = async (dataContext, apiKey) => {
    if (!apiKey) throw new Error("API Key is required");

    const prompt = `
Analyze the following dataset summary and provide 3 short, actionable business insights.
Data Context:
${JSON.stringify(dataContext, null, 2)}

Format the output as a JSON array of objects with keys: "title", "description", "type" (one of: 'positive', 'negative', 'neutral').
RETURN ONLY RAW JSON.
    `.trim();

    try {
        const response = await fetch(`${GEMINI_API_URL}?key=${apiKey}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                contents: [{ parts: [{ text: prompt }] }]
            })
        });

        const data = await response.json();
        if (!response.ok) throw new Error(data.error?.message || "Failed to generate insights");

        let text = data.candidates?.[0]?.content?.parts?.[0]?.text || "[]";
        text = text.replace(/```json/g, '').replace(/```/g, '').trim();
        return JSON.parse(text);
    } catch (error) {
        console.error("Gemini Insight Error:", error);
        throw error;
    }
};
