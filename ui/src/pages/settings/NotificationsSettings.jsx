import React from 'react';
import { Button } from '@/components/ui/button';
import { Row, SectionHeader } from './components';

export default function NotificationsSettings() {
  return (
    <section>
      <SectionHeader
        eyebrow="Notifications"
        title="Stay informed, quietly"
        description="Only the alerts that matter. Delivered in-app and to email."
      />
      <Row title="Query completions" description="Ping when a long-running query or export finishes."><Button variant="outline" size="sm">On</Button></Row>
      <Row title="Weekly digest" description="A Sunday summary of query activity and anomalies."><Button variant="outline" size="sm">Off</Button></Row>
      <Row title="Product updates" description="Monthly highlights of new features and templates."><Button variant="outline" size="sm">Off</Button></Row>
    </section>
  );
}
