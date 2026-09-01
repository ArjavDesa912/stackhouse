import React from 'react';
import { Sparkles } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Row, SectionHeader } from './components';

export default function AboutSettings() {
  return (
    <section>
      <SectionHeader
        eyebrow="About"
        title="StackhouseBrain"
        description="Edge Data OS for sovereign analytics, vector search and visual worksheets."
      />
      <div className="rounded-md border border-border bg-card p-3 shadow-none-whisper">
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-md bg-primary ring-1 ring-primary">
            <Sparkles className="h-4 w-4 text-primary-foreground" />
          </div>
          <div>
            <p className="font-serif text-[20px] font-medium tracking-tight text-foreground">StackhouseBrain v2.0</p>
            <p className="mt-0.5 text-[12px] text-muted-foreground">Build 2026.04.19 · MIT-licensed core</p>
          </div>
        </div>
      </div>
      <Row title="Release channel"><Button variant="outline" size="sm">Stable</Button></Row>
      <Row title="Documentation"><Button variant="outline" size="sm" className="text-[12px]">docs.stackhousebrain.dev</Button></Row>
      <Row title="Support"><Button variant="outline" size="sm" className="text-[12px]">Contact team</Button></Row>
    </section>
  );
}
