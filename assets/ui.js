(() => {
  const coarsePointer = window.matchMedia("(pointer: coarse)");
  const overlaySelector = ".syntaxis-menu-content, .touch-popover";
  const positionTouchOverlay = (overlay) => {
    if (!(overlay instanceof HTMLElement) || !coarsePointer.matches || innerWidth > 768) return;
    const root = overlay.parentElement?.closest("[data-state]");
    const labelledBy = overlay.getAttribute("aria-labelledby");
    const labelledTrigger = labelledBy
      ? root?.querySelector(`[id="${CSS.escape(labelledBy)}"]`)
      : null;
    const controlledBy = overlay.id
      ? root?.querySelector(`[aria-controls="${CSS.escape(overlay.id)}"]`)
      : null;
    const trigger = labelledTrigger ?? controlledBy;
    if (!(trigger instanceof HTMLElement)) return;

    for (const property of ["top", "right", "bottom", "left", "width", "max-width", "max-height"]) {
      overlay.style.removeProperty(property);
    }
    const viewport = window.visualViewport;
    const viewportLeft = viewport?.offsetLeft ?? 0;
    const viewportTop = viewport?.offsetTop ?? 0;
    const viewportWidth = viewport?.width ?? innerWidth;
    const viewportHeight = viewport?.height ?? innerHeight;
    const margin = 8;
    const gap = 6;
    const triggerRect = trigger.getBoundingClientRect();
    const fullWidth = overlay.classList.contains("left-2")
      && overlay.classList.contains("right-2");
    const availableWidth = viewportWidth - margin * 2;
    let overlayWidth = fullWidth ? availableWidth : Math.min(overlay.offsetWidth, availableWidth);
    overlay.style.setProperty("width", `${overlayWidth}px`, "important");
    overlay.style.setProperty("max-width", `${availableWidth}px`, "important");

    const alignLeft = overlay.classList.contains("left-0") && !overlay.classList.contains("right-0");
    const idealLeft = fullWidth
      ? viewportLeft + margin
      : alignLeft
        ? triggerRect.left
        : triggerRect.right - overlayWidth;
    const left = Math.min(
      Math.max(idealLeft, viewportLeft + margin),
      viewportLeft + viewportWidth - overlayWidth - margin,
    );
    overlay.style.setProperty("left", `${Math.round(left)}px`, "important");
    overlay.style.setProperty("right", "auto", "important");
    overlay.style.setProperty("bottom", "auto", "important");

    const below = viewportTop + viewportHeight - triggerRect.bottom - gap - margin;
    const above = triggerRect.top - viewportTop - gap - margin;
    const desiredHeight = Math.min(overlay.scrollHeight, viewportHeight * 0.7);
    const placeBelow = below >= Math.min(desiredHeight, 220) || below >= above;
    const availableHeight = Math.max(80, placeBelow ? below : above);
    const overlayHeight = Math.min(desiredHeight, availableHeight);
    const top = placeBelow
      ? triggerRect.bottom + gap
      : triggerRect.top - gap - overlayHeight;
    overlay.style.setProperty("top", `${Math.round(top)}px`, "important");
    overlay.style.setProperty("max-height", `${Math.round(availableHeight)}px`, "important");
    overlay.dataset.touchPositioned = "true";
  };
  const positionTouchOverlays = () => {
    document.querySelectorAll(overlaySelector).forEach(positionTouchOverlay);
  };
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
      positionTouchOverlays();
    });
  };
  window.visualViewport?.addEventListener("resize", syncViewport, { passive: true });
  window.visualViewport?.addEventListener("scroll", syncViewport, { passive: true });
  coarsePointer.addEventListener?.("change", syncViewport);
  window.addEventListener("orientationchange", syncViewport, { passive: true });
  new MutationObserver((records) => {
    if (records.some(record => [...record.addedNodes].some(node =>
      node instanceof Element && (node.matches(overlaySelector) || node.querySelector(overlaySelector))
    ))) {
      requestAnimationFrame(positionTouchOverlays);
    }
  }).observe(document.documentElement, { childList: true, subtree: true });
  syncViewport();

  // The dropdown primitive prevents pointerdown to keep focus in the menu.
  // WebKit consequently drops both compatibility clicks and native scrolling.
  // Recreate those touch semantics, and provide the same pan fallback for the
  // two drawer lists that WebKit can otherwise strand inside a modal dialog.
  const webkitTouch = navigator.maxTouchPoints > 0
    && CSS.supports?.("-webkit-touch-callout", "none");
  let touchPan = null;
  let suppressTouchClick = null;
  document.addEventListener("pointerdown", (event) => {
    if (event.pointerType !== "touch") return;
    const menu = event.target.closest?.(".syntaxis-menu-content");
    const scrollRegion = event.target.closest?.(".touch-scroll-region");
    const container = menu instanceof HTMLElement
      ? menu
      : webkitTouch && scrollRegion instanceof HTMLElement
        ? scrollRegion
        : null;
    if (!container) return;
    const action = menu instanceof HTMLElement
      ? event.target.closest?.("[role='option'], button, a[href]")
      : null;
    touchPan = {
      container,
      action: action instanceof HTMLElement && menu.contains(action) ? action : null,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      lastY: event.clientY,
      moved: false,
      scrollable: container.scrollHeight > container.clientHeight,
    };
  }, true);
  document.addEventListener("pointermove", (event) => {
    if (!touchPan || event.pointerId !== touchPan.pointerId || !touchPan.scrollable) return;
    const distanceX = Math.abs(event.clientX - touchPan.startX);
    const distanceY = Math.abs(event.clientY - touchPan.startY);
    if (!touchPan.moved && (distanceY < 6 || distanceY <= distanceX)) return;
    touchPan.moved = true;
    touchPan.container.scrollTop += touchPan.lastY - event.clientY;
    touchPan.lastY = event.clientY;
    event.preventDefault();
  }, true);
  const finishTouchPan = (event) => {
    if (!touchPan || event.pointerId !== touchPan.pointerId) return;
    if (touchPan.moved) {
      const container = touchPan.container;
      suppressTouchClick = container;
      window.setTimeout(() => {
        if (suppressTouchClick === container) suppressTouchClick = null;
      }, 500);
    } else if (
      event.type === "pointerup"
      && touchPan.action
      && !touchPan.action.hasAttribute("disabled")
      && touchPan.action.dataset.disabled !== "true"
    ) {
      // HTMLElement.click() produces the click the dropdown's prevented
      // pointerdown suppresses on iOS. Prevent pointerup from adding a second.
      event.preventDefault();
      event.stopImmediatePropagation();
      touchPan.action.click();
    }
    touchPan = null;
  };
  document.addEventListener("pointerup", finishTouchPan, true);
  document.addEventListener("pointercancel", finishTouchPan, true);
  document.addEventListener("click", (event) => {
    if (!suppressTouchClick) return;
    if (suppressTouchClick.contains(event.target)) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
    suppressTouchClick = null;
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
