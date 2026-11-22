import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

export interface TabDefinition {
  id: string;
  label: string;
  badge?: string | number;
  content: ReactNode;
}

export interface OperationalTabsProps {
  tabs: TabDefinition[];
  defaultTabId?: string;
}

export function OperationalTabs({ tabs, defaultTabId }: OperationalTabsProps) {
  const baseId = useId();
  const tablistRef = useRef<HTMLDivElement | null>(null);
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const panelRefs = useRef<Array<HTMLDivElement | null>>([]);

  const [activeId, setActiveId] = useState<string>(defaultTabId ?? tabs[0]?.id ?? "");

  const selectTab = (id: string, focusButton = false, idx?: number) => {
    setActiveId(id);
    const targetIdx = typeof idx === "number" ? idx : tabs.findIndex((t) => t.id === id);
    const tabEl = targetIdx >= 0 ? tabRefs.current[targetIdx] : null;
    if (tabEl) {
      if (focusButton) {
        tabEl.focus();
      }
      if (typeof tabEl.scrollIntoView === "function") {
        try {
          tabEl.scrollIntoView({
            block: "nearest",
            inline: "nearest",
            container: "nearest",
          } as ScrollIntoViewOptions);
        } catch {
          tabEl.scrollIntoView({ block: "nearest", inline: "nearest" });
        }
      }
    }
  };

  useEffect(() => {
    const cleanups: Array<() => void> = [];
    tabs.forEach((tab, index) => {
      const panelEl = panelRefs.current[index];
      if (!panelEl) return;
      const isSelected = tab.id === activeId;
      if (isSelected) {
        panelEl.removeAttribute("hidden");
      } else {
        panelEl.setAttribute("hidden", "until-found");
      }

      const onBeforeMatch = () => {
        setActiveId(tab.id);
      };
      panelEl.addEventListener("beforematch", onBeforeMatch);
      cleanups.push(() => panelEl.removeEventListener("beforematch", onBeforeMatch));
    });
    return () => {
      for (const fn of cleanups) fn();
    };
  }, [activeId, tabs]);

  useEffect(() => {
    const listEl = tablistRef.current;
    if (!listEl || typeof window === "undefined") return;
    if (typeof IntersectionObserver === "undefined") return;
    if (typeof CSS !== "undefined" && CSS.supports?.("animation-timeline", "scroll()")) {
      return;
    }

    const first = listEl.firstElementChild;
    const last = listEl.lastElementChild;
    if (!first || !last) return;

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const overflowed = entry.intersectionRatio < 1;
          if (entry.target === first) {
            listEl.classList.toggle("more-start", overflowed);
          }
          if (entry.target === last) {
            listEl.classList.toggle("more-end", overflowed);
          }
        }
      },
      { root: listEl, threshold: 1, rootMargin: "1px" }
    );

    observer.observe(first);
    observer.observe(last);
    return () => observer.disconnect();
  }, [tabs.length]);

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (tabs.length === 0) return;
    const currentIndex = Math.max(
      0,
      tabs.findIndex((t) => t.id === activeId)
    );
    let nextIndex: number | null = null;
    if (event.key === "ArrowRight") {
      nextIndex = (currentIndex + 1) % tabs.length;
    } else if (event.key === "ArrowLeft") {
      nextIndex = (currentIndex - 1 + tabs.length) % tabs.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = tabs.length - 1;
    }
    if (nextIndex !== null) {
      event.preventDefault();
      selectTab(tabs[nextIndex].id, true, nextIndex);
    }
  };

  return (
    <div className="ops-tabs">
      <div
        ref={tablistRef}
        className="tablist-scroller"
        role="tablist"
        aria-label="Módulos operativos"
        onKeyDown={handleKeyDown}
        {...({ focusgroup: "tablist nomemory" } as Record<string, string>)}
      >
        {tabs.map((tab, index) => {
          const selected = tab.id === activeId;
          const tabDomId = `${baseId}-tab-${tab.id}`;
          return (
            <button
              key={tab.id}
              ref={(el) => {
                tabRefs.current[index] = el;
              }}
              type="button"
              role="tab"
              id={tabDomId}
              className="ops-tab"
              aria-selected={selected}
              tabIndex={selected ? 0 : -1}
              onClick={() => selectTab(tab.id, false, index)}
              {...(selected ? ({ focusgroupstart: "" } as Record<string, string>) : {})}
            >
              <span>{tab.label}</span>
              {tab.badge !== undefined && <span className="ops-tab-badge">{tab.badge}</span>}
            </button>
          );
        })}
      </div>

      <div className="tab-panels-grid">
        {tabs.map((tab, index) => {
          const selected = tab.id === activeId;
          const tabDomId = `${baseId}-tab-${tab.id}`;
          return (
            <div
              key={tab.id}
              ref={(el) => {
                panelRefs.current[index] = el;
              }}
              role="tabpanel"
              aria-labelledby={tabDomId}
              hidden={selected ? undefined : true}
              tabIndex={selected ? 0 : undefined}
              className="ops-tabpanel"
            >
              <div className="tab-panel-body">{tab.content}</div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
