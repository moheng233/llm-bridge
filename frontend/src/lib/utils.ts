import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function focusInScrollArea(element: HTMLElement | null | undefined) {
  if (!element) return;
  let details = element.closest("details");
  while (details) {
    details.open = true;
    details = details.parentElement?.closest("details") ?? null;
  }
  element.focus({ preventScroll: true });
  let area = element.parentElement?.closest<HTMLElement>("[data-scroll-area]");
  while (area) {
    const field = element.getBoundingClientRect();
    const bounds = area.getBoundingClientRect();
    if (field.top < bounds.top + 8) area.scrollTop += field.top - bounds.top - 8;
    else if (field.bottom > bounds.bottom - 8)
      area.scrollTop += Math.min(field.top - bounds.top - 8, field.bottom - bounds.bottom + 8);
    area = area.parentElement?.closest<HTMLElement>("[data-scroll-area]");
  }
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type WithoutChild<T> = T extends { child?: any } ? Omit<T, "child"> : T;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type WithoutChildren<T> = T extends { children?: any } ? Omit<T, "children"> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & {
  ref?: U | null;
};
