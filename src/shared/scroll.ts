export type ScrollCleanup = () => void;

type ScrollNode = HTMLElement;

const isEditableTarget = (target: EventTarget | null): boolean => {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  return Boolean(target.closest("input, textarea, select, [contenteditable='true']"));
};

const wheelDeltaInPixels = (event: WheelEvent): number => {
  if (event.deltaMode === WheelEvent.DOM_DELTA_LINE) {
    return event.deltaY * 16;
  }
  if (event.deltaMode === WheelEvent.DOM_DELTA_PAGE) {
    return event.deltaY * window.innerHeight;
  }
  return event.deltaY;
};

const hasVerticalOverflow = (element: ScrollNode): boolean => element.scrollHeight > element.clientHeight + 1;

const allowsVerticalScroll = (element: ScrollNode): boolean => {
  const overflowY = window.getComputedStyle(element).overflowY;
  return overflowY === "auto" || overflowY === "scroll";
};

const canConsumeDelta = (element: ScrollNode, deltaY: number): boolean => {
  if (!hasVerticalOverflow(element) || !allowsVerticalScroll(element)) {
    return false;
  }
  if (deltaY < 0) {
    return element.scrollTop > 0;
  }
  return element.scrollTop + element.clientHeight < element.scrollHeight - 1;
};

const updateOverflowState = (element: ScrollNode): void => {
  element.dataset.scrollOverflowing = hasVerticalOverflow(element) ? "true" : "false";
};

export const refreshScrollContainers = (root: HTMLElement): void => {
  const containers = [
    ...(root.matches("[data-scroll-container]") ? [root] : []),
    ...Array.from(root.querySelectorAll<ScrollNode>("[data-scroll-container]")),
  ];
  containers.forEach(updateOverflowState);
};

const scrollCandidates = (root: HTMLElement, target: EventTarget | null): ScrollNode[] => {
  if (!(target instanceof Node) || !root.contains(target)) {
    return [];
  }
  const candidates: ScrollNode[] = [];
  let node: Node | null = target;
  while (node && node !== root.parentElement) {
    if (node instanceof HTMLElement && (node === root || node.matches("[data-scroll-container]") || hasVerticalOverflow(node))) {
      candidates.push(node);
    }
    node = node.parentNode;
  }
  if (!candidates.includes(root)) {
    candidates.push(root);
  }
  return candidates;
};

export const installScrollBehavior = (root: HTMLElement): ScrollCleanup => {
  let frame: number | null = null;
  const observed = new Set<ScrollNode>();
  const resizeObserver = typeof ResizeObserver === "undefined"
    ? null
    : new ResizeObserver((entries) => {
      entries.forEach((entry) => updateOverflowState(entry.target as ScrollNode));
    });

  const observe = (): void => {
    const containers = [
      ...(root.matches("[data-scroll-container]") ? [root] : []),
      ...Array.from(root.querySelectorAll<ScrollNode>("[data-scroll-container]")),
    ];
    containers.forEach((element) => {
      updateOverflowState(element);
      if (!observed.has(element)) {
        observed.add(element);
        resizeObserver?.observe(element);
      }
    });
  };

  const scheduleObserve = (): void => {
    if (frame !== null) {
      return;
    }
    frame = window.requestAnimationFrame(() => {
      frame = null;
      observe();
    });
  };

  const onWheel = (event: WheelEvent): void => {
    if (event.defaultPrevented || isEditableTarget(event.target)) {
      return;
    }
    const deltaY = wheelDeltaInPixels(event);
    if (Math.abs(deltaY) < 0.01 || event.shiftKey || Math.abs(event.deltaX) > Math.abs(event.deltaY)) {
      return;
    }
    const target = scrollCandidates(root, event.target).find((element) => canConsumeDelta(element, deltaY));
    if (!target) {
      // No child can consume this direction. Do not preventDefault: the browser
      // can continue the wheel chain at the page/window boundary.
      return;
    }
    event.preventDefault();
    target.scrollTop += deltaY;
    updateOverflowState(target);
  };

  const mutationObserver = typeof MutationObserver === "undefined"
    ? null
    : new MutationObserver(scheduleObserve);

  observe();
  mutationObserver?.observe(root, { childList: true, subtree: true, characterData: true });
  root.addEventListener("wheel", onWheel, { capture: true, passive: false });

  return () => {
    root.removeEventListener("wheel", onWheel, true);
    mutationObserver?.disconnect();
    resizeObserver?.disconnect();
    if (frame !== null) {
      window.cancelAnimationFrame(frame);
      frame = null;
    }
    observed.clear();
  };
};
