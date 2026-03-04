import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

type BankOption = {
  id: string;
  label: string;
};

type QuoteValue = {
  value: number | null;
  digits: number | null;
  unit: string;
  showName: string;
  quotedAt: string | null;
};

type BankSnapshot = {
  bankId: string;
  bankLabel: string;
  cny: QuoteValue;
  usd: QuoteValue;
};

type WidgetConfig = {
  refreshMs: number;
  banks: BankOption[];
};

type Snapshot = {
  refreshedAt: string;
  data: BankSnapshot[];
};

const bankNameEl = document.getElementById("bankName") as HTMLDivElement;
const priceCnyEl = document.getElementById("priceCny") as HTMLParagraphElement;
const priceUsdEl = document.getElementById("priceUsd") as HTMLParagraphElement;
const metaEl = document.getElementById("meta") as HTMLElement;
const prevBtn = document.getElementById("prevBtn") as HTMLButtonElement;
const nextBtn = document.getElementById("nextBtn") as HTMLButtonElement;
const minBtn = document.getElementById("minBtn") as HTMLButtonElement;
const closeBtn = document.getElementById("closeBtn") as HTMLButtonElement;
const titlebarEl = document.querySelector(".titlebar") as HTMLElement;
const dragTopEl = document.getElementById("dragTop") as HTMLDivElement;

const appWindow = getCurrentWindow();

let banks: BankOption[] = [];
let selected = 0;
let refreshMs = 5000;
let snapshot: Snapshot | null = null;

function formatNumber(
  value: number | null | undefined,
  digits: number | null | undefined,
): string {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return "--";
  }
  const precision =
    Number.isInteger(digits) && (digits as number) >= 0 && (digits as number) <= 6
      ? (digits as number)
      : 2;
  return value.toFixed(precision);
}

function formatTime(isoTime: string | null | undefined): string {
  if (!isoTime) {
    return "--";
  }
  const dt = new Date(isoTime);
  if (Number.isNaN(dt.getTime())) {
    return "--";
  }
  const hh = `${dt.getHours()}`.padStart(2, "0");
  const mm = `${dt.getMinutes()}`.padStart(2, "0");
  const ss = `${dt.getSeconds()}`.padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}

function currentBank(): BankOption | null {
  return banks[selected] ?? null;
}

function renderBankName(): void {
  const bank = currentBank();
  bankNameEl.textContent = bank ? bank.label : "--";
}

function renderCurrentPrices(): void {
  renderBankName();
  const bank = currentBank();
  if (!bank) {
    priceCnyEl.textContent = "--";
    priceUsdEl.textContent = "--";
    metaEl.textContent = "未配置银行";
    return;
  }

  const row = snapshot?.data?.find((item) => item.bankId === bank.id);
  if (!row) {
    priceCnyEl.textContent = "--";
    priceUsdEl.textContent = "--";
    metaEl.textContent = "该银行暂无数据";
    return;
  }

  priceCnyEl.textContent = formatNumber(row.cny.value, row.cny.digits);
  priceUsdEl.textContent = formatNumber(row.usd.value, row.usd.digits);
  metaEl.textContent = `刷新 ${formatTime(snapshot?.refreshedAt)} · ${row.cny.showName}`;
}

async function refreshSnapshot(): Promise<void> {
  try {
    snapshot = await invoke<Snapshot>("get_latest");
    renderCurrentPrices();
  } catch (error) {
    renderBankName();
    priceCnyEl.textContent = "--";
    priceUsdEl.textContent = "--";
    metaEl.textContent = `获取失败: ${String(error)}`;
  }
}

function switchBank(step: number): void {
  if (!banks.length) {
    return;
  }
  selected = (selected + step + banks.length) % banks.length;
  renderCurrentPrices();
}

async function init(): Promise<void> {
  const config = await invoke<WidgetConfig>("get_config");
  banks = Array.isArray(config.banks) ? config.banks : [];
  refreshMs = Number(config.refreshMs) || 5000;

  renderCurrentPrices();
  await refreshSnapshot();

  window.setInterval(() => {
    void refreshSnapshot();
  }, refreshMs);
}

function shouldIgnoreDragStart(target: EventTarget | null): boolean {
  return target instanceof Element && Boolean(target.closest("button"));
}

function emitDebug(scope: string, message: string): void {
  const line = `[${new Date().toISOString()}][${scope}] ${message}`;
  console.info(line);
  void invoke("debug_log", { scope, message }).catch((error) => {
    console.warn(`debug_log invoke failed: ${String(error)}`);
  });
}

function bindDrag(el: HTMLElement, tag: string): void {
  el.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || shouldIgnoreDragStart(event.target)) {
      return;
    }

    event.preventDefault();
    emitDebug("drag", `${tag} mousedown x=${event.clientX}, y=${event.clientY}`);

    void appWindow
      .startDragging()
      .then(() => emitDebug("drag", `${tag} startDragging success`))
      .catch((error) => {
        emitDebug("drag", `${tag} startDragging failed: ${String(error)}`);
      });
  });
}

prevBtn.addEventListener("click", () => switchBank(-1));
nextBtn.addEventListener("click", () => switchBank(1));
minBtn.addEventListener("click", () => {
  void invoke("hide_main_window");
});
closeBtn.addEventListener("click", () => {
  void invoke("exit_app");
});

bindDrag(titlebarEl, "titlebar");
bindDrag(dragTopEl, "dragTop");

void init();
