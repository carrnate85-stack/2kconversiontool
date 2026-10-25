(function(root) {
  class VirtualTable {
    body;
    viewport;
    renderRow;
    rowHeight;
    overscan;
    columns;
    items;
    frame;
    observer;
    constructor(body, renderRow, { rowHeight = 42, overscan = 8, columns = 5 } = {}) {
      this.body = body;
      this.viewport = body.closest(".native-hair-table-wrap");
      this.renderRow = renderRow;
      this.rowHeight = rowHeight;
      this.overscan = overscan;
      this.columns = columns;
      this.items = [];
      this.frame = 0;
      this.viewport.addEventListener("scroll", () => this.schedule(), { passive: true });
      this.observer = new ResizeObserver(() => this.schedule());
      this.observer.observe(this.viewport);
    }
    schedule() {
      if (!this.frame) this.frame = requestAnimationFrame(() => {
        this.frame = 0;
        this.render();
      });
    }
    setItems(items, reset = false) {
      this.items = items;
      if (reset) this.viewport.scrollTop = 0;
      this.render();
    }
    spacer(height) {
      const row = document.createElement("tr");
      row.className = "virtual-spacer";
      row.setAttribute("aria-hidden", "true");
      const cell = document.createElement("td");
      cell.colSpan = this.columns;
      cell.style.height = `${height}px`;
      row.append(cell);
      return row;
    }
    render() {
      const header = this.body.parentElement.tHead?.offsetHeight || 0;
      const count = this.items.length;
      const visible = Math.ceil(Math.max(0, this.viewport.clientHeight - header) / this.rowHeight);
      const first = Math.min(Math.max(0, count - visible), Math.floor(this.viewport.scrollTop / this.rowHeight));
      const start = Math.max(0, first - this.overscan);
      const end = Math.min(count, first + visible + this.overscan);
      const focused = this.body.contains(document.activeElement) ? document.activeElement.closest("tr")?.dataset.assetId : null;
      const fragment = document.createDocumentFragment();
      fragment.append(this.spacer(start * this.rowHeight));
      for (let i = start; i < end; i += 1) {
        const row = this.renderRow(this.items[i]);
        row.setAttribute("aria-rowindex", String(i + 2));
        fragment.append(row);
      }
      fragment.append(this.spacer((count - end) * this.rowHeight));
      this.body.replaceChildren(fragment);
      this.body.parentElement.setAttribute("aria-rowcount", String(count + 1));
      if (focused) [...this.body.rows].find((row) => row.dataset.assetId === focused)?.focus({ preventScroll: true });
    }
  }
  root.VirtualTable = VirtualTable;
})(window);
