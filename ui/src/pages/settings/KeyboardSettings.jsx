import React from 'react';
import { Row, SectionHeader } from './components';

export default function KeyboardSettings() {
  return (
    <section>
      <SectionHeader
        eyebrow="Keyboard"
        title="Flow faster"
        description="Shortcuts for power users. Modify any binding inline."
      />
      {[
        ['Toggle sidebar', '⌘', 'B'],
        ['Quick jump', '⌘', 'K'],
        ['Save worksheet', '⌘', 'S'],
        ['Run query', '⌘', 'Enter'],
        ['Undo', '⌘', 'Z'],
        ['Redo', '⌘⇧', 'Z'],
        ['Presentation mode', '⌘⇧', 'P'],
      ].map(([action, mod, key]) => (
        <Row key={action} title={action}>
          <div className="inline-flex items-center gap-0.5">
            <kbd className="inline-flex h-5 min-w-5 items-center justify-center rounded-md border border-border bg-muted px-0.5.5 font-mono text-[11px] text-foreground">{mod}</kbd>
            <kbd className="inline-flex h-5 min-w-5 items-center justify-center rounded-md border border-border bg-muted px-0.5.5 font-mono text-[11px] text-foreground">{key}</kbd>
          </div>
        </Row>
      ))}
    </section>
  );
}
