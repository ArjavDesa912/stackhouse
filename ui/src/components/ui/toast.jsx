import { Toaster as SonnerToaster, toast } from 'sonner';
import { useTheme } from '@/app/ThemeProvider';

export function Toaster() {
  const { resolvedTheme } = useTheme();
  return (
    <SonnerToaster
      theme={resolvedTheme}
      richColors
      closeButton
      position="bottom-right"
      toastOptions={{
        classNames: {
          toast:
            'group toast border-border bg-card text-foreground rounded-md shadow-none text-body-sm',
          description: 'text-muted-foreground',
          actionButton: 'bg-primary text-primary-foreground',
          cancelButton: 'bg-muted text-muted-foreground',
        },
      }}
    />
  );
}

export { toast };
