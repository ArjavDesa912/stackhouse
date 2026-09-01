import { mergeProps } from "@base-ui/react/merge-props"
import { useRender } from "@base-ui/react/use-render"
import { cva } from "class-variance-authority";

import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "group/badge inline-flex h-4 w-fit shrink-0 items-center justify-center gap-0.5 overflow-hidden rounded-full border border-transparent px-1.5 py-0.5 text-[11px] font-medium tracking-[0.04em] whitespace-nowrap transition-all focus-visible:border-ring focus-visible:ring-[2px] focus-visible:ring-ring/40 has-data-[icon=inline-end]:pr-0.5.5 has-data-[icon=inline-start]:pl-0.5.5 aria-invalid:border-destructive aria-invalid:ring-destructive/20 [&>svg]:pointer-events-none [&>svg]:size-3!",
  {
    variants: {
      variant: {
        default: "bg-secondary text-secondary-foreground shadow-none dark:shadow-none",
        brand: "bg-primary/10 text-primary border-primary/25",
        secondary:
          "bg-muted text-muted-foreground [a]:hover:bg-secondary",
        destructive:
          "bg-destructive/10 text-destructive focus-visible:ring-destructive/20 [a]:hover:bg-destructive/20",
        outline:
          "border-border text-foreground [a]:hover:bg-muted",
        ghost:
          "hover:bg-muted hover:text-foreground",
        link: "text-primary underline-offset-4 hover:underline",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

function Badge({
  className,
  variant = "default",
  render,
  ...props
}) {
  return useRender({
    defaultTagName: "span",
    props: mergeProps({
      className: cn(badgeVariants({ variant }), className),
    }, props),
    render,
    state: {
      slot: "badge",
      variant,
    },
  });
}

export { Badge, badgeVariants }
