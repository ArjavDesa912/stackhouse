import React from 'react';
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

export function ExplainData({ contextData }) {
    return (
        <div className="bg-card/40 backdrop-blur-3xl border-2 border-primary/20 rounded-[3rem] p-4 shadow-none animate-in slide-in-from-bottom-5 duration-700">
            <div className="flex items-center justify-between mb-4 px-1">
                <h3 className="text-base font-semibold text-foreground flex items-center gap-2 tracking-tight uppercase italic">
                    <div className="w-2 h-2 rounded-full bg-primary animate-pulse ring-2 ring-primary/80"></div>
                    Causal_Engine: Insights
                </h3>
                <Badge variant="outline" className="bg-primary/5 text-primary border-primary/20 font-semibold uppercase text-[9px] px-2 tracking-widest">Data_Audit</Badge>
            </div>
            
            <div className="space-y-3">
                <div className="p-3 bg-background/60 border border-border rounded-[2rem] group hover:border-destructive/30 transition-all">
                    <div className="flex justify-between items-start mb-1">
                        <span className="text-xs font-semibold text-foreground/80 uppercase tracking-tight">Geo_Segment: <strong className="text-foreground">Neo-Tokyo</strong></span>
                        <span className="text-destructive font-semibold text-sm italic tracking-tight">-22.4% VOL</span>
                    </div>
                    <p className="text-[11px] text-muted-foreground font-medium leading-relaxed">
                        Entropy spikes detected in <strong className="text-foreground uppercase italic px-0.5">Logistics_Hub</strong> (α-9). Anomalous weather patterns correlated (ρ=0.88).
                    </p>
                </div>
                <div className="p-3 bg-background/60 border border-border rounded-[2rem] group hover:border-success/30 transition-all">
                    <div className="flex justify-between items-start mb-1">
                        <span className="text-xs font-semibold text-foreground/80 uppercase tracking-tight">Entity: <strong className="text-foreground">Sovereign_04</strong></span>
                        <span className="text-success font-semibold text-sm italic tracking-tight">+$48.2k ARR</span>
                    </div>
                    <p className="text-[11px] text-muted-foreground font-medium leading-relaxed">
                        Expansion event triggered by <strong className="text-foreground uppercase italic px-0.5">Neural_Api</strong> usage. Tier-upgrade propensity increased to 94%.
                    </p>
                </div>
            </div>
            <Button className="w-full mt-3 h-12 rounded-lg bg-primary text-primary-foreground font-semibold text-[10px] uppercase tracking-[0.2em] shadow-none shadow-none hover:scale-[1.02] transition-all">
                Execute deep_causal_recursive_chain
            </Button>
        </div>
    );
}
