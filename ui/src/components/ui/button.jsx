import { Button as ButtonPrimitive } from "@base-ui/react/button"
import { cva } from "class-variance-authority";

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "group/button inline-flex shrink-0 items-center justify-center rounded-md border border-transparent bg-clip-padding text-sm font-medium tracking-tight whitespace-nowrap transition-all outline-none select-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/40 active:not-aria-[haspopup]:translate-y-px disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default:
          "bg-primary text-primary-foreground shadow-none hover:bg-primary/90",
        outline:
          "border-border bg-transparent text-foreground hover:bg-muted aria-expanded:bg-muted dark:border-border",
        secondary:
          "bg-secondary text-secondary-foreground shadow-none hover:bg-muted aria-expanded:bg-muted dark:shadow-none",
        ghost:
          "hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground",
        destructive:
          "bg-destructive/10 text-destructive hover:bg-destructive/20 focus-visible:border-destructive/40 focus-visible:ring-destructive/20 dark:bg-destructive/20 dark:hover:bg-destructive/30",
        link: "text-primary underline-offset-4 hover:underline",
      },
      size: {
        default:
          "h-6 gap-0.5.5 px-1.5 has-data-[icon=inline-end]:pr-1 has-data-[icon=inline-start]:pl-1",
        xs: "h-5 gap-0.5 rounded-[min(var(--radius-md),10px)] px-1 text-xs in-data-[slot=button-group]:rounded-md has-data-[icon=inline-end]:pr-0.5.5 has-data-[icon=inline-start]:pl-0.5.5 [&_svg:not([class*='size-'])]:size-3",
        sm: "h-6 gap-0.5 rounded-[min(var(--radius-md),12px)] px-1.5 text-[0.8rem] in-data-[slot=button-group]:rounded-md has-data-[icon=inline-end]:pr-0.5.5 has-data-[icon=inline-start]:pl-0.5.5 [&_svg:not([class*='size-'])]:size-3.5",
        lg: "h-8 gap-0.5.5 px-1.5 has-data-[icon=inline-end]:pr-1 has-data-[icon=inline-start]:pl-1",
        icon: "size-8",
        "icon-xs":
          "size-6 rounded-[min(var(--radius-md),10px)] in-data-[slot=button-group]:rounded-md [&_svg:not([class*='size-'])]:size-3",
        "icon-sm":
          "size-7 rounded-[min(var(--radius-md),12px)] in-data-[slot=button-group]:rounded-md",
        "icon-lg": "size-9",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Button({
  className,
  variant = "default",
  size = "default",
  ...props
}) {
  return (
    <ButtonPrimitive
      data-slot="button"
      className={cn(buttonVariants({ variant, size, className }))}
      {...props} />
  );
}

export { Button, buttonVariants }
