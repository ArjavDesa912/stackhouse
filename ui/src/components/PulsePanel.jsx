import React, { useEffect, useState } from 'react';
import {
    Sparkles, TrendingUp, TrendingDown, AlertCircle,
    ArrowUpRight, ArrowDownRight, Activity, Calendar, Key, Loader2, Lightbulb, Zap, BrainCircuit
} from 'lucide-react';
import _ from 'lodash';
import { generateInsights } from '../services/aiService';
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { apiGet } from '@/lib/apiClient';

export default function PulsePanel({ tableName }) {
    const [insights, setInsights] = useState([]);
    const [loading, setLoading] = useState(false);
    const [apiKey, setApiKey] = useState(localStorage.getItem('stackhouse_ai_key') || '');
    const [aiLoading, setAiLoading] = useState(false);

    useEffect(() => {
        if (!tableName) return;

        const analyzeData = async () => {
            setLoading(true);
            try {
                // Fetch a sample for analysis
                const res = await apiGet(`/v1/query/${tableName}?limit=1000`);
                const json = res.data;

                if (json.success && json.data.length > 0) {
                    const data = json.data;
                    const headers = Object.keys(data[0]);
                    const measures = headers.filter(h => typeof data[0][h] === 'number');
                    const dimensions = headers.filter(h => typeof data[0][h] === 'string');

                    const newInsights = [];

                    // 1. Basic Heuristics (Instant)
                    measures.forEach(m => {
                        const total = _.sumBy(data, m);
                        newInsights.push({
                            type: 'summary',
                            title: `Total ${_.startCase(m)}`,
                            value: total.toLocaleString(undefined, { maximumFractionDigits: 0 }),
                            metric: m,
                            icon: Activity
                        });
                    });

                    // 2. High Extremes / Outliers
                    measures.forEach(m => {
                        const maxItem = _.maxBy(data, m);
                        if (maxItem) {
                            const dim = dimensions[0] ? maxItem[dimensions[0]] : 'Record';
                            newInsights.push({
                                type: 'outlier',
                                icon: AlertCircle,
                                title: `Peak ${_.startCase(m)}`,
                                description: `${dim}: ${maxItem[m].toLocaleString()}`
                            });
                        }
                    });

                    setInsights(newInsights);

                    // 3. AI Deep Analysis (if key exists)
                    if (apiKey) {
                        setAiLoading(true);
                        // Summary context for AI
                        const summaryContext = {
                            tableName,
                            rowCount: data.length,
                            columns: headers,
                            sample: data.slice(0, 5),
                            stats: measures.map(m => ({
                                column: m,
                                total: _.sumBy(data, m),
                                avg: _.meanBy(data, m)
                            }))
                        };

                        try {
                            const aiResults = await generateInsights(summaryContext, apiKey);
                            const formattedAi = aiResults.map(r => ({
                                type: 'ai_insight',
                                title: r.title,
                                description: r.description,
                                icon: Lightbulb,
                                trend: r.type === 'positive' ? 'up' : r.type === 'negative' ? 'down' : null
                            }));
                            setInsights(prev => [...formattedAi, ...prev]);
                        } catch (e) {
                            console.error("AI Insight failed", e);
                        } finally {
                            setAiLoading(false);
                        }
                    }
                }
            } catch (err) {
                console.error("Pulse error", err);
            } finally {
                setLoading(false);
            }
        };

        analyzeData();
    }, [tableName, apiKey]); // Re-run if key changes

    if (!tableName) return (
        <div className="w-72 bg-background border-l border-border flex flex-col h-full animate-in slide-in-from-right duration-500 shadow-none items-center justify-center p-4 text-center">
            <div className="w-12 h-12 rounded-[2rem] bg-muted/30 border border-border flex items-center justify-center mb-3 text-muted-foreground/30">
                <BrainCircuit className="w-6 h-6" />
            </div>
            <h4 className="text-sm font-bold mb-1">Neural Engine Offline</h4>
            <p className="text-xs text-muted-foreground">Select a dataset to begin real-time pattern synthesis and AI deep-dives.</p>
        </div>
    );

    return (
        <div className="w-72 bg-card/50 backdrop-blur-3xl border-l border-border flex flex-col h-full animate-in slide-in-from-right duration-500 z-10 shadow-none relative overflow-hidden">
            <div className="p-3 border-b border-border flex items-center justify-between bg-background/40">
                <div className="flex items-center gap-2">
                    <div className="relative">
                        <Sparkles className="w-4 h-4 text-primary" />
                        <div className="absolute -top-1 -right-1 w-1 h-1 bg-primary rounded-full animate-ping"></div>
                    </div>
                    <h3 className="font-semibold text-foreground tracking-tight">AI Pulse</h3>
                </div>
                {aiLoading && <Loader2 className="w-3 h-3 text-primary animate-spin" />}
            </div>

            <div className="p-3 bg-muted/20 border-b border-border space-y-2">
                <div className="flex items-center justify-between">
                    <span className="text-[9px] font-bold text-muted-foreground uppercase tracking-widest">Model Configuration</span>
                    <Badge variant="outline" className="text-[8px] h-3 px-0.5.5 font-bold bg-background/50 border-border">GEMINI-1.5</Badge>
                </div>
                <div className="relative group">
                    <Key className="w-2.5 h-2.5 text-muted-foreground absolute left-3 top-1/2 -translate-y-1/2 transition-colors group-focus-within:text-primary" />
                    <Input
                        type="password"
                        value={apiKey}
                        onChange={e => {
                            setApiKey(e.target.value);
                            localStorage.setItem('stackhouse_ai_key', e.target.value);
                        }}
                        placeholder="Neural API Key..."
                        className="pl-9 h-8 bg-background/50 border-border rounded-md text-xs focus-visible:ring-primary/20 transition-all font-mono"
                    />
                </div>
            </div>

            <div className="flex-1 overflow-y-auto p-3 space-y-3 custom-scrollbar">
                {loading ? (
                    <div className="flex flex-col items-center justify-center py-6 gap-3 opacity-40">
                        <Activity className="w-8 h-8 text-primary animate-pulse" />
                        <span className="text-[10px] font-bold uppercase tracking-[0.2em]">Deconstructing Data...</span>
                    </div>
                ) : (
                    insights.map((insight, i) => (
                        <InsightCard key={i} data={insight} index={i} />
                    ))
                )}
                {insights.length === 0 && !loading && (
                    <div className="flex flex-col items-center justify-center py-5 text-center gap-3 opacity-50">
                        <Zap className="w-6 h-6 text-muted-foreground/30" />
                        <p className="text-[10px] font-bold uppercase tracking-widest leading-relaxed">No anomalies detected in the current buffer.</p>
                    </div>
                )}
            </div>
            
            <div className="p-3 bg-muted/10 border-t border-border mt-auto">
                <div className="flex items-center gap-1 text-[9px] font-bold text-muted-foreground tracking-tight">
                    <div className="w-0.5 h-0.5 rounded-full bg-success"></div>
                    SYSTEM OPERATIONAL • {new Date().toLocaleTimeString()}
                </div>
            </div>
        </div>
    );
}

function InsightCard({ data, index }) {
    const isAi = data.type === 'ai_insight';
    return (
        <div 
            className={`p-3 rounded-[1.75rem] border transition-all duration-300 group relative animate-in slide-in-from-right-4 fade-in ${
                isAi 
                ? 'bg-primary/5 border-primary/20 hover:border-primary/40 hover:bg-primary/10 shadow-none shadow-none-primary/5' 
                : 'bg-background border-border hover:border-primary/20 hover:shadow-none'
            }`}
            style={{ animationDelay: `${index * 100}ms` }}
        >
            <div className="flex items-start justify-between mb-3">
                <Badge 
                    variant="outline" 
                    className={`text-[9px] font-semibold uppercase tracking-[0.1em] px-1 py-0 h-4 border-none ${
                        isAi ? 'bg-primary/10 text-primary' : 'bg-muted text-muted-foreground'
                    }`}
                >
                    {data.type === 'ai_insight' ? 'AI insight' : data.type}
                </Badge>
                <div className="flex items-center gap-0.5.5">
                    {data.trend === 'up' && <TrendingUp className="w-3 h-3 text-success" />}
                    {data.trend === 'down' && <TrendingDown className="w-3 h-3 text-destructive" />}
                    {data.icon && <data.icon className={`w-3 h-3 ${isAi ? 'text-primary' : 'text-primary opacity-60'}`} />}
                </div>
            </div>

            <div className="mb-1">
                <h4 className="text-[13px] font-bold text-foreground leading-snug group-hover:text-primary transition-colors">{data.title}</h4>
            </div>

            {data.value && (
                <div className="text-xl font-serif font-medium text-foreground tracking-tight mb-1 tabular-nums">
                    {data.value}
                </div>
            )}

            {data.description && (
                <p className="text-[11px] text-muted-foreground leading-relaxed font-medium">
                    {data.description}
                </p>
            )}
            
            {isAi && (
                <div className="absolute bottom-3 right-3 opacity-20 group-hover:opacity-100 transition-opacity">
                    <Sparkles className="w-2 h-2 text-primary" />
                </div>
            )}
        </div>
    );
}
