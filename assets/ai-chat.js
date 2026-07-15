(() => {
  const mounted = new WeakSet();

  const mount = (container) => {
    if (mounted.has(container)) return;
    mounted.add(container);
    let stickToEnd = true;

    container.addEventListener("scroll", () => {
      stickToEnd = container.scrollHeight - container.scrollTop - container.clientHeight < 96;
    }, { passive: true });

    new MutationObserver(() => {
      if (stickToEnd) container.scrollTop = container.scrollHeight;
    }).observe(container, { childList: true, subtree: true, characterData: true });

    container.scrollTop = container.scrollHeight;
  };

  const discover = () => {
    document.querySelectorAll("[data-agent-scroll]").forEach(mount);
  };

  new MutationObserver(discover).observe(document.documentElement, { childList: true, subtree: true });
  discover();
})();
