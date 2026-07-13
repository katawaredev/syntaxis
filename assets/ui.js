(() => {
  const isFocusable = (element) => {
    if (!(element instanceof HTMLElement) || element.tabIndex < 0 || element.hasAttribute("disabled")) {
      return false;
    }
    if (element instanceof HTMLAnchorElement) return Boolean(element.href);
    if (element instanceof HTMLInputElement) return element.type !== "hidden";
    return element.matches("button, select, textarea, [tabindex]");
  };

  class FocusTrap {
    constructor(container) {
      this.container = container;
      this.restoreFocusElement = document.activeElement;
      this.focusables = () => [...this.container.querySelectorAll("*")].filter(isFocusable);
      this.keydown = (event) => {
        if (event.key !== "Tab") return;
        const focusables = this.focusables();
        if (focusables.length === 0) return;
        const current = focusables.indexOf(document.activeElement);
        const offset = event.shiftKey ? -1 : 1;
        const next = (current + offset + focusables.length) % focusables.length;
        focusables[next].focus();
        event.preventDefault();
      };
      this.container.addEventListener("keydown", this.keydown);
      this.focusables()[0]?.focus();
    }

    remove() {
      this.container.removeEventListener("keydown", this.keydown);
      if (this.restoreFocusElement instanceof HTMLElement && this.restoreFocusElement.isConnected) {
        this.restoreFocusElement.focus();
      }
    }
  }

  window.createFocusTrap = (container) => new FocusTrap(container);

})();
