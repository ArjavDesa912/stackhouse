import { Component } from 'react';
import { AlertOctagon, RefreshCcw } from 'lucide-react';
import { Button } from '@/components/ui/button';

export class ErrorBoundary extends Component {
  constructor(props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error) {
    return { hasError: true, error };
  }

  componentDidCatch(error, info) {
    if (import.meta.env.DEV) console.error('ErrorBoundary caught:', error, info);
    
    // Wire error boundary to POST /v1/log service
    import('@/lib/apiClient').then(({ apiPost }) => {
      apiPost('/v1/log', {
        level: 'error',
        message: error.message,
        stack: error.stack,
        componentStack: info.componentStack
      }).catch(err => console.error('Failed to report error to log service:', err));
    });
  }

  reset = () => this.setState({ hasError: false, error: null });

  render() {
    if (!this.state.hasError) return this.props.children;
    if (this.props.fallback) {
      return this.props.fallback({ error: this.state.error, reset: this.reset });
    }
    return (
      <div className="flex h-full w-full items-center justify-center bg-background p-4">
        <div className="max-w-md rounded-md border border-border bg-card p-3 shadow-none">
          <div className="mb-3 inline-flex h-8 w-8 items-center justify-center rounded-md bg-destructive/10 text-destructive">
            <AlertOctagon className="h-4 w-4" />
          </div>
          <h3 className="text-display-sm text-foreground">Something went wrong.</h3>
          <p className="text-body-sm text-muted-foreground mt-0.5.5">
            {this.state.error?.message || 'An unexpected error occurred while rendering this view.'}
          </p>
          <div className="mt-3 flex gap-1">
            <Button onClick={this.reset} variant="default" size="sm">
              <RefreshCcw className="h-2.5 w-2.5" />
              Retry
            </Button>
            <Button onClick={() => window.location.reload()} variant="outline" size="sm">
              Reload page
            </Button>
          </div>
        </div>
      </div>
    );
  }
}
