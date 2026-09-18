import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-sm font-mono text-[11px] font-medium tracking-wide uppercase transition-[background-color,color,border-color,opacity,transform] duration-150 ease-out disabled:pointer-events-none disabled:opacity-40 active:not-disabled:scale-[0.96] select-none",
  {
    variants: {
      variant: {
        default:
          "border border-accent/40 bg-accent/10 text-accent hover:bg-accent/20",
        solid: "border border-accent bg-accent text-bg hover:opacity-90",
        ghost: "border border-transparent text-muted hover:text-fg hover:bg-fg/5",
        danger:
          "border border-danger/50 bg-danger/10 text-danger hover:bg-danger/20",
        rec: "border border-danger/60 bg-danger/15 text-danger hover:bg-danger/25",
      },
      size: {
        default: "h-10 px-3 min-h-10",
        sm: "h-8 px-2.5 min-h-8",
        icon: "size-10 min-h-10",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";

export { Button, buttonVariants };
