import type { ButtonHTMLAttributes, ReactNode } from "react";
import { Icon, type IconName } from "./Icon";

/** default/success/warning/danger/clear/highlight are the classic builder command-button variants; flat = text button used in flyouts. */
export type ButtonVariant = "default" | "primary" | "danger" | "ghost" | "success" | "warning" | "clear" | "highlight" | "flat";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  small?: boolean;
  icon?: IconName;
  children?: ReactNode;
}

export function Button({ variant = "default", small, icon, children, className = "", type = "button", ...rest }: ButtonProps) {
  const cls = ["btn", variant !== "default" && `btn-${variant}`, small && "btn-sm", className].filter(Boolean).join(" ");
  return (
    <button type={type} className={cls} {...rest}>
      {icon && <Icon name={icon} />}
      {children}
    </button>
  );
}

/** Icon-only button; `label` is required for accessibility. */
export function IconButton({ icon, label, variant = "ghost", ...rest }: Omit<ButtonProps, "children" | "icon"> & { icon: IconName; label: string }) {
  return (
    <Button variant={variant} className="btn-icon" aria-label={label} title={label} {...rest}>
      <Icon name={icon} />
    </Button>
  );
}
