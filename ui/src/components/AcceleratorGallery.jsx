import React, { useState } from 'react';
import { BarChart2, Target, Megaphone, Truck, Users, Server, DollarSign, ShoppingCart, Sparkles, X, Search } from 'lucide-react';
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";

// Each template's header block is filled solid with its `color` token, so the
// overlay content needs a matching readable-on-top color per tone. Only
// bg-primary and bg-foreground have a token designed for exactly this
// (primary-foreground / background, the latter being foreground's own theme
// inverse). success/destructive have no such pairing in the token system —
// white/black are kept there deliberately, verified against the actual
// success/destructive hex values rather than assumed.
const HERO_CONTENT = {
    'bg-primary': { text: 'text-primary-foreground', muted: 'text-primary-foreground/70', tint: 'bg-primary-foreground/20', border: 'border-primary-foreground/30' },
    'bg-foreground': { text: 'text-background', muted: 'text-background/70', tint: 'bg-background/20', border: 'border-background/30' },
};
const HERO_CONTENT_DEFAULT = { text: 'text-white', muted: 'text-white/70', tint: 'bg-white/20', border: 'border-white/30' };

export function AcceleratorGallery({ onClose, onApply }) {
    const [selectedCategory, setSelectedCategory] = useState('All');
    const [searchQuery, setSearchQuery] = useState('');
    const [applying, setApplying] = useState(null);

    const categories = ['All', 'Executive', 'Sales', 'Marketing', 'Operations', 'HR', 'IT', 'Finance', 'Retail', 'Education'];

    const templates = [
        { id: 1, title: "EXECUTIVE_WAR_ROOM", category: "Executive", color: "bg-primary", icon: BarChart2, description: "Mission-critical KPI oversight for sovereign-level decision making.", kpis: ["Core Rev", "Burn", "LTV"], config: { theme: 'high-contrast' } },
        { id: 2, title: "SALES_NEURAL_FUNNEL", category: "Sales", color: "bg-success", icon: Target, description: "Predictive lead-to-close intelligence using vector similarity.", kpis: ["Deal Velocity", "Win Prob"], config: { theme: 'emerald' } },
        { id: 3, title: "CAMPAIGN_ROI_NUCLEUS", category: "Marketing", color: "bg-primary", icon: Megaphone, description: "Cross-platform attribution and automated budget allocation.", kpis: ["CAC", "ROAS"], config: { theme: 'fuchsia' } },
        { id: 4, title: "LOGISTICS_SYMPHONY", category: "Operations", color: "bg-primary", icon: Truck, description: "Real-time global supply chain tracking with anomaly alerts.", kpis: ["OTIF", "OOS"], config: { theme: 'amber' } },
        { id: 5, title: "WORKFORCE_DYNAMICS", category: "HR", color: "bg-destructive", icon: Users, description: "Cognitive attrition prediction and diversity matrix analysis.", kpis: ["NPS", "FTE"], config: { theme: 'rose' } },
        { id: 6, title: "INFRA_PULSE_MONITOR", category: "IT", color: "bg-foreground", icon: Server, description: "Low-latency telemetry for edge computing cluster health.", kpis: ["Uptime", "Latency"], config: { theme: 'slate' } },
        { id: 7, title: "TREASURY_QUANT_KIT", category: "Finance", color: "bg-success", icon: DollarSign, description: "Deep financial auditing and cash flow volatility modeling.", kpis: ["EBITDA", "Runway"], config: { theme: 'emerald' } },
        { id: 8, title: "RETAIL_CART_GENIUS", category: "Retail", color: "bg-primary", icon: ShoppingCart, description: "Basket affinity analysis and checkout friction mapping.", kpis: ["AUR", "UPT"], config: { theme: 'yellow' } },
    ];

    const filteredTemplates = templates.filter(t =>
        (selectedCategory === 'All' || t.category === selectedCategory) &&
        (t.title.toLowerCase().includes(searchQuery.toLowerCase()) || t.description.toLowerCase().includes(searchQuery.toLowerCase()))
    );

    const handleApply = (template) => {
        setApplying(template.id);
        setTimeout(() => {
            if (onApply) onApply(template.config);
            setApplying(null);
            onClose();
        }, 1500);
    }

    return (
        <div className="fixed inset-0 bg-background/80 backdrop-blur-2xl flex items-center justify-center z-[100] p-5 select-none animate-in fade-in duration-500">
            <div className="bg-card/40 border border-border w-full max-w-7xl h-[85vh] rounded-[4rem] shadow-none flex flex-col overflow-hidden relative">
                <div className="absolute top-0 right-0 w-0.5/2 h-0.5/2 bg-primary/5 rounded-full blur-[150px] -z-10 pointer-events-none"></div>
                
                {/* Header */}
                <div className="px-5 py-5 border-b border-border flex justify-between items-center group/header">
                    <div>
                        <div className="flex items-center gap-3 mb-1">
                            <Sparkles className="w-6 h-6 text-primary animate-pulse" />
                            <h2 className="text-3xl font-serif font-medium text-foreground tracking-tight uppercase italic">STACKHOUSED_MARKET</h2>
                            <Badge className="h-5 px-3 bg-primary text-primary-foreground border-none rounded-full font-semibold text-[10px] uppercase tracking-widest shadow-none shadow-none">PREMIUM_ACCESS</Badge>
                        </div>
                        <p className="text-sm text-muted-foreground font-bold uppercase tracking-[0.2em] ml-5">Accelerate Intelligence with Curated Neural Blueprints</p>
                    </div>
                    <Button onClick={onClose} variant="ghost" size="icon" className="h-12 w-12 rounded-full hover:bg-destructive/10 text-muted-foreground hover:text-destructive transition-all">
                        <X className="w-6 h-6" />
                    </Button>
                </div>

                <div className="flex flex-1 overflow-hidden">
                    {/* Sidebar */}
                    <div className="w-72 bg-muted/20 border-r border-border p-5 flex flex-col gap-2 overflow-y-auto custom-scrollbar">
                        <div className="text-[10px] font-semibold text-muted-foreground uppercase mb-3 tracking-[0.3em] px-2">Filter Systems</div>
                        {categories.map(cat => (
                            <button
                                key={cat}
                                onClick={() => setSelectedCategory(cat)}
                                className={`flex items-center justify-between px-3 py-3 rounded-[1.75rem] text-xs font-semibold uppercase tracking-widest transition-all duration-500 ${selectedCategory === cat ? 'bg-primary text-primary-foreground shadow-none shadow-none-primary/30 scale-105' : 'text-muted-foreground hover:bg-background/80 hover:text-foreground'}`}
                            >
                                <span>{cat}</span>
                                {selectedCategory === cat && <div className="w-1 h-1 rounded-full bg-primary-foreground animate-pulse"></div>}
                            </button>
                        ))}
                    </div>

                    {/* Main */}
                    <div className="flex-1 flex flex-col pt-5 px-5">
                        <div className="relative mb-5">
                            <Search className="absolute left-6 top-1/2 -translate-y-1/2 w-5 h-5 text-muted-foreground" />
                            <Input
                                type="text"
                                placeholder="SEARCH_NEURAL_NETS..."
                                value={searchQuery}
                                onChange={(e) => setSearchQuery(e.target.value)}
                                className="h-14 bg-background/50 border-2 border-border rounded-[2rem] pl-6 pr-4 text-base font-semibold tracking-widest uppercase placeholder:text-muted-foreground/30 focus:ring-4 focus:ring-primary/10 transition-all"
                            />
                        </div>

                        <div className="flex-1 overflow-y-auto pb-6 custom-scrollbar grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 content-start">
                            {filteredTemplates.map(t => {
                                const hero = HERO_CONTENT[t.color] || HERO_CONTENT_DEFAULT;
                                return (
                                <div key={t.id} className="group bg-background/40 border border-border rounded-[3rem] overflow-hidden shadow-none hover:shadow-none hover:border-primary/30 hover:-translate-y-2 transition-all duration-700 flex flex-col cursor-pointer relative">
                                    <div className={`h-28 ${t.color} p-4 flex flex-col justify-between relative`}>
                                        <div className="absolute inset-0 opacity-10 flex items-center justify-center grayscale">
                                            <t.icon className="w-40 h-40 scale-150 rotate-12" />
                                        </div>
                                        <div className="flex justify-between items-start relative z-10">
                                            <div className={`w-10 h-10 ${hero.tint} backdrop-blur-xl rounded-lg flex items-center justify-center border ${hero.border} shadow-none`}>
                                                <t.icon className={`w-5 h-5 ${hero.text}`} />
                                            </div>
                                            {applying === t.id && (
                                                <div className="flex items-center gap-1 px-3 py-0.5.5 bg-black/40 backdrop-blur-3xl rounded-full text-[10px] font-semibold text-white italic tracking-widest animate-pulse">
                                                    INITIALIZING...
                                                </div>
                                            )}
                                        </div>
                                        <div className="relative z-10">
                                            <span className={`text-[10px] font-semibold uppercase tracking-[0.3em] ${hero.muted}`}>{t.category}</span>
                                            <h3 className={`text-lg font-semibold ${hero.text} tracking-tight uppercase italic`}>{t.title}</h3>
                                        </div>
                                    </div>

                                    <div className="p-4 flex-1 flex flex-col justify-between">
                                        <p className="text-xs text-muted-foreground font-medium leading-relaxed mb-3 group-hover:text-foreground transition-colors">{t.description}</p>
                                        
                                        <div>
                                            <div className="flex flex-wrap gap-1 mb-4">
                                                {t.kpis.map(k => (
                                                    <Badge key={k} variant="outline" className="h-5 px-2 bg-muted/20 border-border rounded-full text-[9px] font-semibold tracking-widest text-muted-foreground group-hover:bg-primary/5 group-hover:text-primary transition-all uppercase">{k}</Badge>
                                                ))}
                                            </div>
                                            <Button 
                                                onClick={() => handleApply(t)}
                                                disabled={applying !== null}
                                                className="w-full h-12 rounded-[1.5rem] bg-muted group-hover:bg-primary text-muted-foreground group-hover:text-primary-foreground font-semibold text-[10px] uppercase tracking-[0.2em] transition-all"
                                            >
                                                Deploy Module
                                            </Button>
                                        </div>
                                    </div>
                                </div>
                                );
                            })}
                        </div>
                    </div>
                </div>
            </div>
        </div>
    );
}
