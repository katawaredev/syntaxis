(() => {
  const coarsePointer = window.matchMedia("(pointer: coarse)");
  let viewportFrame = null;
  const syncViewport = () => {
    if (viewportFrame !== null) cancelAnimationFrame(viewportFrame);
    viewportFrame = requestAnimationFrame(() => {
      viewportFrame = null;
      if (coarsePointer.matches && window.visualViewport) {
        document.documentElement.style.setProperty(
          "--app-height",
          `${Math.round(window.visualViewport.height)}px`,
        );
      } else {
        document.documentElement.style.removeProperty("--app-height");
      }
    });
  };
  window.visualViewport?.addEventListener("resize", syncViewport, { passive: true });
  window.visualViewport?.addEventListener("scroll", syncViewport, { passive: true });
  coarsePointer.addEventListener?.("change", syncViewport);
  window.addEventListener("orientationchange", syncViewport, { passive: true });
  syncViewport();

  // The dropdown primitive prevents pointerdown to keep focus in the menu.
  // That also cancels native touch scrolling, so reproduce only the missing
  // vertical pan while leaving taps and non-touch pointers to the primitive.
  let menuPan = null;
  let suppressMenuClick = null;
  document.addEventListener("pointerdown", (event) => {
    if (event.pointerType !== "touch") return;
    const menu = event.target.closest?.(".syntaxis-menu-content");
    if (!(menu instanceof HTMLElement) || menu.scrollHeight <= menu.clientHeight) return;
    menuPan = {
      menu,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      lastY: event.clientY,
      moved: false,
    };
  }, true);
  document.addEventListener("pointermove", (event) => {
    if (!menuPan || event.pointerId !== menuPan.pointerId) return;
    const distanceX = Math.abs(event.clientX - menuPan.startX);
    const distanceY = Math.abs(event.clientY - menuPan.startY);
    if (!menuPan.moved && (distanceY < 6 || distanceY <= distanceX)) return;
    menuPan.moved = true;
    menuPan.menu.scrollTop += menuPan.lastY - event.clientY;
    menuPan.lastY = event.clientY;
  }, true);
  const finishMenuPan = (event) => {
    if (!menuPan || event.pointerId !== menuPan.pointerId) return;
    if (menuPan.moved) suppressMenuClick = menuPan.menu;
    menuPan = null;
  };
  document.addEventListener("pointerup", finishMenuPan, true);
  document.addEventListener("pointercancel", finishMenuPan, true);
  document.addEventListener("click", (event) => {
    if (!suppressMenuClick) return;
    const menu = event.target.closest?.(".syntaxis-menu-content");
    if (menu === suppressMenuClick) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
    suppressMenuClick = null;
  }, true);

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
