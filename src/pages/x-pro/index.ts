import type { Cleanup } from "../../contracts/types";
import type { Translate } from "../../i18n";
import { escapeHtml, on, qs } from "../../shared/dom";

interface GridRow {
  id: number;
  name: string;
  department: string;
  salary: number;
  age: number;
}

type ColumnKey = "name" | "department" | "salary" | "age";

const demoRows: GridRow[] = [
  { id: 1, name: "Alice", department: "Engineering", salary: 95000, age: 28 },
  { id: 2, name: "Bob", department: "Engineering", salary: 105000, age: 32 },
  { id: 3, name: "Carol", department: "Design", salary: 88000, age: 26 },
  { id: 4, name: "Dave", department: "Design", salary: 92000, age: 30 },
  { id: 5, name: "Eve", department: "Marketing", salary: 78000, age: 25 },
  { id: 6, name: "Frank", department: "Marketing", salary: 82000, age: 35 },
  { id: 7, name: "Grace", department: "Engineering", salary: 115000, age: 40 },
  { id: 8, name: "Hank", department: "Design", salary: 97000, age: 33 },
];

export interface XProPageProps {
  t: Translate;
}

export const mountXProPage = (container: HTMLElement, props: XProPageProps): Cleanup => {
  let filter = "";
  let sortKey: ColumnKey = "name";
  let sortDirection: "asc" | "desc" = "asc";
  let currentPage = 1;
  const pageSize = 5;
  const selected = new Set<number>();
  const expandedDetails = new Set<number>();
  const collapsedGroups = new Set<string>();
  const visibleColumns = new Set<ColumnKey>(["name", "department", "salary", "age"]);
  const rowOrder = demoRows.map((row) => row.id);
  let draggedId: number | null = null;
  let grouping = false;
  let disposed = false;
  let resizeObserver: ResizeObserver | null = null;

  const columns: Array<{ key: ColumnKey; label: string }> = [
    { key: "name", label: props.t("xPro.columnName") },
    { key: "department", label: props.t("xPro.columnDepartment") },
    { key: "salary", label: props.t("xPro.columnSalary") },
    { key: "age", label: props.t("xPro.columnAge") },
  ];

  const formatCurrency = (value: number): string => `$${value.toLocaleString()}`;

  const orderedRows = (): GridRow[] => rowOrder.map((id) => demoRows.find((row) => row.id === id)).filter((row): row is GridRow => row !== undefined);

  const filteredRows = (): GridRow[] => {
    const query = filter.toLowerCase();
    const rows = orderedRows().filter((row) => `${row.name} ${row.department}`.toLowerCase().includes(query));
    return rows.sort((left, right) => {
      const a = left[sortKey];
      const b = right[sortKey];
      const result = a > b ? 1 : a < b ? -1 : 0;
      return sortDirection === "asc" ? result : -result;
    });
  };

  const cell = (row: GridRow, key: ColumnKey): string => {
    if (!visibleColumns.has(key)) {
      return "";
    }
    const value = key === "salary" ? formatCurrency(row.salary) : String(row[key]);
    return `<td data-column-key="${key}" class="${key === "name" ? "data-grid__pinned" : ""}">${key === "name" ? `<strong>${escapeHtml(value)}</strong>` : escapeHtml(value)}</td>`;
  };

  const renderRow = (row: GridRow): string => `
    <tr draggable="true" data-row-id="${row.id}" class="${selected.has(row.id) ? "is-selected" : ""}">
      <td class="data-grid__drag"><span aria-hidden="true">::</span><input type="checkbox" data-row-select="${row.id}" aria-label="${escapeHtml(row.name)}" ${selected.has(row.id) ? "checked" : ""}/></td>
      ${cell(row, "name")}${cell(row, "department")}${cell(row, "salary")}${cell(row, "age")}
      <td><button class="button button--small" data-detail="${row.id}" aria-expanded="${expandedDetails.has(row.id)}">${expandedDetails.has(row.id) ? props.t("xPro.collapse") : props.t("xPro.details")}</button></td>
    </tr>
    ${expandedDetails.has(row.id) ? `<tr class="data-grid__detail"><td colspan="6"><strong>${escapeHtml(row.name)}</strong> · ${escapeHtml(row.department)} · ${formatCurrency(row.salary)} · ${row.age}</td></tr>` : ""}
  `;

  const renderGroupedRows = (rows: GridRow[]): string => {
    const groups = new Map<string, GridRow[]>();
    rows.forEach((row) => groups.set(row.department, [...(groups.get(row.department) ?? []), row]));
    return Array.from(groups.entries()).map(([department, group]) => {
      const average = Math.round(group.reduce((sum, row) => sum + row.salary, 0) / group.length);
      const collapsed = collapsedGroups.has(department);
      return `<tr class="data-grid__group"><td colspan="6"><button data-group-toggle="${escapeHtml(department)}" aria-expanded="${!collapsed}">${collapsed ? props.t("xPro.expand") : props.t("xPro.collapse")}</button><strong>${escapeHtml(department)}</strong><span>${group.length} · ${props.t("xPro.aggregation")}: ${formatCurrency(average)}</span></td></tr>${collapsed ? "" : group.map(renderRow).join("")}`;
    }).join("");
  };

  const renderGrid = (): string => {
    const rows = filteredRows();
    const totalPages = Math.max(1, Math.ceil(rows.length / pageSize));
    currentPage = Math.min(currentPage, totalPages);
    const pageRows = grouping ? rows : rows.slice((currentPage - 1) * pageSize, currentPage * pageSize);
    const visibleHeader = columns.map((column) => visibleColumns.has(column.key) ? `<th data-column-key="${column.key}" class="${column.key === "name" ? "data-grid__pinned" : ""}"><button class="table-sort" data-sort="${column.key}">${escapeHtml(column.label)}${sortKey === column.key ? ` <small>${sortDirection === "asc" ? "^" : "v"}</small>` : ""}</button>${column.key === "name" ? `<small class="muted">${props.t("xPro.pinned")}</small>` : ""}</th>` : "").join("");
    return `<div class="grid-toolbar"><input data-grid-filter placeholder="${escapeHtml(props.t("xPro.filterPlaceholder"))}" value="${escapeHtml(filter)}"/><label class="select-field"><span>${props.t("xPro.columns")}</span><select data-column-toggle multiple size="1">${columns.map((column) => `<option value="${column.key}" ${visibleColumns.has(column.key) ? "selected" : ""}>${escapeHtml(column.label)}</option>`).join("")}</select></label><label class="toggle-inline"><input type="checkbox" data-grouping ${grouping ? "checked" : ""}/><span>${props.t("xPro.groupByDepartment")}</span></label><button class="button button--small" data-export-csv>${props.t("xPro.exportCsv")}</button><span class="grid-selection">${props.t("xPro.selected", { count: selected.size })}</span></div>
      <div class="data-grid-wrap" data-scroll-container="data-grid"><table class="data-grid"><thead><tr><th><input type="checkbox" data-select-all aria-label="${escapeHtml(props.t("xPro.gridTitle"))}"/></th>${visibleHeader}<th>${props.t("common.actions")}</th></tr></thead><tbody>${pageRows.length ? grouping ? renderGroupedRows(pageRows) : pageRows.map(renderRow).join("") : `<tr><td colspan="6"><div class="empty-state">${props.t("xPro.noRows")}</div></td></tr>`}</tbody></table></div>
      <div class="grid-footer"><span>${props.t("xPro.dragHint")}</span><span>${props.t("xPro.pageOf", { page: currentPage, total: totalPages })}</span><div class="button-row"><button class="button button--small" data-page="prev" ${currentPage <= 1 || grouping ? "disabled" : ""}>${props.t("xPro.previous")}</button><button class="button button--small" data-page="next" ${currentPage >= totalPages || grouping ? "disabled" : ""}>${props.t("xPro.next")}</button></div></div>`;
  };

  const renderCharts = (): string => `<div class="chart-grid" data-chart-grid>
    <article class="chart-card"><h3>${props.t("xPro.heatmap")}</h3><p class="chart-caption">${props.t("xPro.chartResponsive")}</p><div class="heatmap">${Array.from({ length: 20 }, (_, index) => `<span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: `D${index + 1}`, value: (index * 7) % 100 }))}" style="--heat:${(index * 13) % 100}%"></span>`).join("")}</div></article>
    <article class="chart-card"><h3>${props.t("xPro.funnel")}</h3><div class="funnel"><span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartInput"), value: 10000 }))}" style="width:100%">10000</span><span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartReview"), value: 5200 }))}" style="width:78%">5200</span><span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartShip"), value: 1400 }))}" style="width:45%">1400</span></div></article>
    <article class="chart-card"><h3>${props.t("xPro.radar")}</h3><svg class="chart-svg" viewBox="0 0 200 160" role="img" aria-label="${escapeHtml(props.t("xPro.radar"))}"><polygon points="100,12 168,62 142,140 58,140 32,62" fill="none" stroke="var(--color-border)"/><polygon points="100,35 143,67 126,116 74,116 57,67" fill="none" stroke="var(--color-border)"/><polygon data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.radar"), value: 82 }))}" points="100,30 151,68 130,124 70,114 53,65" fill="var(--color-accentSoft)" stroke="var(--color-accent)"/></svg></article>
    <article class="chart-card"><h3>${props.t("xPro.candlestick")}</h3><svg class="chart-svg" viewBox="0 0 240 160" role="img" aria-label="${escapeHtml(props.t("xPro.candlestick"))}"><path d="M10 126H230M10 86H230M10 46H230" stroke="var(--color-border)"/><g data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.candlestick"), value: 116 }))}" fill="var(--color-success)" stroke="var(--color-success)"><path d="M28 116V72M20 88h16v24H20zM68 104V44M60 62h16v32H60zM148 112V52M140 70h16v28H140zM198 93V34M190 48h16v28H190z"/></g><g fill="var(--color-danger)" stroke="var(--color-danger)"><path d="M48 124V66M40 78h16v36H40zM108 98V30M100 46h16v34H100zM178 122V62M170 78h16v26H170z"/></g></svg></article>
    <article class="chart-card"><h3>${props.t("xPro.sankey")}</h3><div class="sankey"><span class="sankey-node" data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartInput"), value: 850 }))}">${props.t("xPro.chartInput")}</span><i></i><span class="sankey-node" data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartReview"), value: 580 }))}">${props.t("xPro.chartReview")}</span><i></i><span class="sankey-node" data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartShip"), value: 420 }))}">${props.t("xPro.chartShip")}</span></div></article>
    <article class="chart-card"><h3>${props.t("xPro.gantt")}</h3><div class="gantt"><span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartResearch"), value: "01/01–01/14" }))}" style="width:72%"></span><span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartDesign"), value: "01/10–01/25" }))}" style="width:45%"></span><span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartDevelopment"), value: "01/20–02/15" }))}" style="width:86%"></span><span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartTesting"), value: "02/10–02/28" }))}" style="width:30%"></span></div></article>
    <article class="chart-card chart-card--wide"><h3>${props.t("xPro.treemap")}</h3><div class="treemap"><span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartCore"), value: 450 }))}" style="flex:5">${props.t("xPro.chartCore")}</span><span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartUi"), value: 200 }))}" style="flex:3">${props.t("xPro.chartUi")}</span><span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartOps"), value: 100 }))}" style="flex:2">${props.t("xPro.chartOps")}</span><span data-chart-point data-tooltip="${escapeHtml(props.t("xPro.chartTooltip", { label: props.t("xPro.chartDocs"), value: 80 }))}" style="flex:1">${props.t("xPro.chartDocs")}</span></div></article>
  </div><div class="chart-tooltip" data-chart-tooltip hidden></div>`;

  const render = (): void => {
    if (disposed) {
      return;
    }
    container.innerHTML = `<section class="page page--xpro"><div class="page-header"><span class="eyebrow">${props.t("xPro.eyebrow")}</span><h1>${props.t("xPro.title")}</h1><p>${props.t("xPro.description")}</p></div><section class="card"><div class="section-heading"><h2>${props.t("xPro.gridTitle")}</h2></div><div data-grid-root>${renderGrid()}</div></section>${renderCharts()}</section>`;
  };

  const exportCsv = (): void => {
    const rows = filteredRows();
    const header = columns.filter((column) => visibleColumns.has(column.key)).map((column) => column.label).join(",");
    const body = rows.map((row) => columns.filter((column) => visibleColumns.has(column.key)).map((column) => JSON.stringify(column.key === "salary" ? row.salary : row[column.key])).join(",")).join("\n");
    const blob = new Blob([`${header}\n${body}`], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = "x-pro-demo.csv";
    anchor.click();
    URL.revokeObjectURL(url);
  };

  const refreshGrid = (): void => {
    const grid = qs<HTMLElement>(container, "[data-grid-root]");
    if (grid) {
      grid.innerHTML = renderGrid();
    }
  };

  render();
  const cleanupInput = on(container, "input", (event) => {
    const target = event.target;
    if (target instanceof HTMLInputElement && target.dataset.gridFilter !== undefined) {
      filter = target.value;
      currentPage = 1;
      refreshGrid();
    }
  });
  const cleanupClick = on(container, "click", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement) || disposed) {
      return;
    }
    const sort = target.closest<HTMLElement>("[data-sort]")?.dataset.sort as ColumnKey | undefined;
    if (sort) {
      sortDirection = sortKey === sort && sortDirection === "desc" ? "asc" : "desc";
      sortKey = sort;
      refreshGrid();
      return;
    }
    if (target.closest("[data-export-csv]")) {
      exportCsv();
      return;
    }
    const page = target.closest<HTMLElement>("[data-page]")?.dataset.page;
    if (page === "prev") currentPage = Math.max(1, currentPage - 1);
    if (page === "next") currentPage += 1;
    if (page) {
      refreshGrid();
      return;
    }
    const group = target.closest<HTMLElement>("[data-group-toggle]")?.dataset.groupToggle;
    if (group) {
      if (collapsedGroups.has(group)) collapsedGroups.delete(group); else collapsedGroups.add(group);
      refreshGrid();
      return;
    }
    const detail = target.closest<HTMLElement>("[data-detail]")?.dataset.detail;
    if (detail) {
      const id = Number(detail);
      if (expandedDetails.has(id)) expandedDetails.delete(id); else expandedDetails.add(id);
      refreshGrid();
      return;
    }
    const rowId = target instanceof HTMLInputElement ? target.dataset.rowSelect : undefined;
    if (rowId && target instanceof HTMLInputElement) {
      const id = Number(rowId);
      if (target.checked) selected.add(id); else selected.delete(id);
      refreshGrid();
      return;
    }
    if (target instanceof HTMLInputElement && target.dataset.selectAll !== undefined) {
      const rows = filteredRows();
      if (target.checked) rows.forEach((row) => selected.add(row.id)); else rows.forEach((row) => selected.delete(row.id));
      refreshGrid();
    }
  });
  const cleanupChange = on(container, "change", (event) => {
    const target = event.target;
    if (target instanceof HTMLSelectElement && target.dataset.columnToggle !== undefined) {
      visibleColumns.clear();
      Array.from(target.selectedOptions).forEach((option) => visibleColumns.add(option.value as ColumnKey));
      refreshGrid();
    }
    if (target instanceof HTMLInputElement && target.dataset.grouping !== undefined) {
      grouping = target.checked;
      currentPage = 1;
      refreshGrid();
    }
  });
  const cleanupDragStart = on(container, "dragstart", (event) => {
    const target = event.target;
    if (target instanceof HTMLElement) {
      const row = target.closest<HTMLElement>("[data-row-id]")?.dataset.rowId;
      draggedId = row ? Number(row) : null;
    }
  });
  const cleanupDragOver = on(container, "dragover", (event) => {
    if (draggedId !== null) event.preventDefault();
  });
  const cleanupDrop = on(container, "drop", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement) || draggedId === null) {
      return;
    }
    event.preventDefault();
    const destination = Number(target.closest<HTMLElement>("[data-row-id]")?.dataset.rowId);
    const from = rowOrder.indexOf(draggedId);
    const to = rowOrder.indexOf(destination);
    if (from >= 0 && to >= 0 && from !== to) {
      rowOrder.splice(from, 1);
      rowOrder.splice(to, 0, draggedId);
      refreshGrid();
    }
    draggedId = null;
  });
  const cleanupTooltip = on(container, "mouseover", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) return;
    const point = target.closest<HTMLElement>("[data-chart-point]");
    const tooltip = qs<HTMLElement>(container, "[data-chart-tooltip]");
    if (!point || !tooltip) return;
    tooltip.textContent = point.dataset.tooltip ?? "";
    tooltip.hidden = false;
    const bounds = container.getBoundingClientRect();
    const pointBounds = point.getBoundingClientRect();
    tooltip.style.left = `${pointBounds.left - bounds.left + pointBounds.width / 2}px`;
    tooltip.style.top = `${pointBounds.top - bounds.top - 8}px`;
  });
  const cleanupTooltipOut = on(container, "mouseout", (event) => {
    const target = event.target;
    if (target instanceof HTMLElement && target.closest("[data-chart-point]")) {
      qs<HTMLElement>(container, "[data-chart-tooltip]")?.setAttribute("hidden", "true");
    }
  });
  if (typeof ResizeObserver !== "undefined") {
    resizeObserver = new ResizeObserver((entries) => {
      if (!disposed && entries[0]) {
        container.dataset.chartSize = entries[0].contentRect.width < 640 ? "compact" : "wide";
      }
    });
    resizeObserver.observe(container);
  }

  return () => {
    disposed = true;
    cleanupInput(); cleanupClick(); cleanupChange(); cleanupDragStart(); cleanupDragOver(); cleanupDrop(); cleanupTooltip(); cleanupTooltipOut();
    resizeObserver?.disconnect();
    container.replaceChildren();
  };
};
