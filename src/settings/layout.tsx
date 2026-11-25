import type { ReactNode } from "react";

export function PageHeader({ title, description }: { title: string; description?: string }) {
  return (
    <header className="page-header" data-tauri-drag-region>
      <h1 data-tauri-drag-region>{title}</h1>
      {description && <p data-tauri-drag-region>{description}</p>}
    </header>
  );
}

export function Group({ title, children, footer }: { title?: string; children: ReactNode; footer?: ReactNode }) {
  return (
    <section className="group">
      {title && <h2 className="group-title">{title}</h2>}
      <div className="group-body">{children}</div>
      {footer && <div className="group-footer">{footer}</div>}
    </section>
  );
}

export function Row({
  title,
  description,
  children,
  htmlFor,
}: {
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  htmlFor?: string;
}) {
  return (
    <div className="row">
      <span className="row-text">
        {htmlFor ? (
          <label className="row-title" htmlFor={htmlFor}>
            {title}
          </label>
        ) : (
          <span className="row-title">{title}</span>
        )}
        {description && <span className="row-desc">{description}</span>}
      </span>
      {children !== undefined && <div className="row-actions">{children}</div>}
    </div>
  );
}
