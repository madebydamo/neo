// Neo app telemetry for Node apps.
//
// Copied from neo `templates/plugin/telemetry/node/neo-telemetry.mjs`; keep the
// copy unchanged so every app exports the same names (docs/APP-TELEMETRY.md).
// Needs `prom-client`.
//
//   import * as tel from "./neo-telemetry.mjs";
//   tel.setupLogging();                 // console.* → JSON lines, errors counted
//   tel.startMetrics();                 // :$NEO_METRICS_PORT/metrics, only when set
//   const call = tel.llmCall("openrouter", model, "assistant");
//   try { const r = await fetch(...); call.usage = (await r.json()).usage; }
//   catch (e) { call.fail(e); throw e; } finally { call.end(); }
//   tel.feature("export_pdf"); tel.login("entra", true, user); tel.seenUser(user);
//   await tel.job("nightly", async () => { ... }); tel.dataUpdated("gps");
import http from "node:http";
import client from "prom-client";

const reg = client.register;
const C = (name, help, labelNames) => new client.Counter({ name, help, labelNames });
const G = (name, help, labelNames) => new client.Gauge({ name, help, labelNames });
const H = (name, help, labelNames, buckets) => new client.Histogram({ name, help, labelNames, buckets });

export const LLM_REQUESTS = C("neo_app_llm_requests_total", "AI calls by result.", ["provider", "model", "feature", "status"]);
export const LLM_TOKENS = C("neo_app_llm_tokens_total", "AI tokens; kind=input is uncached input.", ["provider", "model", "feature", "kind"]);
export const LLM_COST = C("neo_app_llm_cost_usd_total", "AI cost in USD as reported by the provider.", ["provider", "model", "feature"]);
export const LLM_DURATION = H("neo_app_llm_request_duration_seconds", "Time for one AI call.", ["provider", "model", "feature"], [0.5, 1, 2, 5, 10, 20, 30, 60, 120, 300, 600]);
export const LOGINS = C("neo_app_logins_total", "Sign-ins by result.", ["method", "result"]);
export const FEATURES = C("neo_app_feature_used_total", "Uses of a feature.", ["feature"]);
export const ERRORS = C("neo_app_errors_total", "Errors by component and kind.", ["component", "kind"]);
export const JOB_RUNS = C("neo_app_job_runs_total", "Background job runs.", ["job", "status"]);
export const JOB_LAST_SUCCESS = G("neo_app_job_last_success_timestamp_seconds", "Last successful run.", ["job"]);
export const JOB_LAST_RUN = G("neo_app_job_last_run_timestamp_seconds", "Last run, whether it succeeded or not.", ["job"]);
export const JOB_DURATION = H("neo_app_job_duration_seconds", "Background job run time.", ["job"], [1, 5, 15, 60, 300, 900, 3600, 14400]);
export const DATA_UPDATED = G("neo_app_data_updated_timestamp_seconds", "Last update of a data source.", ["source"]);
export const BUILD_INFO = G("neo_app_build_info", "App version.", ["version"]);

// ── Logging ──────────────────────────────────────────────────────────────────

const LEVELS = { debug: 10, info: 20, warning: 30, error: 40, critical: 50 };
let minLevel = LEVELS.info;

function errFields(e) {
  return e instanceof Error ? { error: e.name, exc: e.stack || String(e) } : {};
}

/** One JSON line: ts, level, logger, msg, fields (an Error in fields.err → exc). */
export function log(level, msg, fields = {}, logger = "app") {
  if ((LEVELS[level] ?? 20) < minLevel) return;
  const { err, ...rest } = fields;
  const line = { ts: new Date().toISOString(), level, logger, msg, ...rest, ...errFields(err) };
  process.stdout.write(JSON.stringify(line) + "\n");
  if (LEVELS[level] >= LEVELS.error) ERRORS.labels(logger, err instanceof Error ? err.name : "log").inc();
}

export const logger = (name) => ({
  debug: (m, f) => log("debug", m, f, name),
  info: (m, f) => log("info", m, f, name),
  warn: (m, f) => log("warning", m, f, name),
  error: (m, f) => log("error", m, f, name),
});

/** Route console.* through log() and log crashes. */
export function setupLogging(level = process.env.LOG_LEVEL || "info") {
  minLevel = LEVELS[level.toLowerCase()] ?? LEVELS.info;
  const fmt = (args) => {
    const err = args.find((a) => a instanceof Error);
    const msg = args.filter((a) => !(a instanceof Error)).map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" ");
    return [msg || (err ? err.message : ""), err ? { err } : {}];
  };
  console.debug = (...a) => log("debug", ...fmt(a), "console");
  console.log = console.info = (...a) => log("info", ...fmt(a), "console");
  console.warn = (...a) => log("warning", ...fmt(a), "console");
  console.error = (...a) => log("error", ...fmt(a), "console");
  process.on("uncaughtException", (err) => { log("critical", "uncaught exception", { err }, "unhandled"); process.exit(1); });
  process.on("unhandledRejection", (err) => log("error", "unhandled rejection", { err: err instanceof Error ? err : new Error(String(err)) }, "unhandled"));
}

// ── Metrics endpoint ─────────────────────────────────────────────────────────

/** Serve /metrics on $NEO_METRICS_PORT (Neo sets it only while monitoring collects). */
export function startMetrics(port = Number(process.env.NEO_METRICS_PORT || 0)) {
  if (!port) return false;
  http.createServer(async (req, res) => {
    if (req.url !== "/metrics") { res.writeHead(404).end(); return; }
    res.writeHead(200, { "Content-Type": reg.contentType }).end(await reg.metrics());
  }).listen(port, "0.0.0.0");
  log("info", `metrics on :${port}`, {}, "neo_telemetry");
  return true;
}

export const buildInfo = (version) => BUILD_INFO.labels(version || "unknown").set(1);

// ── AI calls ─────────────────────────────────────────────────────────────────

const n = (v) => Number(v) || 0;

/** {input (uncached), cache_read, cache_write, output} from Anthropic, OpenAI/OpenRouter or Gemini usage. */
export function normalizeUsage(u) {
  if (!u) return {};
  if (u.input_tokens !== undefined || u.cache_read_input_tokens !== undefined) {
    return { input: n(u.input_tokens), cache_read: n(u.cache_read_input_tokens), cache_write: n(u.cache_creation_input_tokens), output: n(u.output_tokens) };
  }
  if (u.prompt_tokens !== undefined) {
    const cached = n(u.prompt_tokens_details?.cached_tokens), write = n(u.prompt_tokens_details?.cache_write_tokens);
    return { input: Math.max(n(u.prompt_tokens) - cached - write, 0), cache_read: cached, cache_write: write, output: n(u.completion_tokens) };
  }
  if (u.promptTokenCount !== undefined) {
    const cached = n(u.cachedContentTokenCount);
    return { input: Math.max(n(u.promptTokenCount) - cached, 0), cache_read: cached, cache_write: 0, output: n(u.candidatesTokenCount) };
  }
  return {};
}

/** ok|error|timeout|rate_limited|auth from an error or HTTP status. */
export function classifyError(err, status) {
  const code = Number(status ?? err?.status ?? err?.statusCode ?? err?.response?.status) || 0;
  const name = String(err?.name || "").toLowerCase();
  if (code === 401 || code === 403 || code === 402 || name.includes("auth")) return "auth";
  if (code === 429 || name.includes("ratelimit")) return "rate_limited";
  if (code === 408 || code === 504 || name.includes("timeout") || name === "aborterror") return "timeout";
  return "error";
}

export function recordLlm(provider, model, feature, usage, { status = "ok", duration, costUsd } = {}) {
  model = model || "unknown";
  LLM_REQUESTS.labels(provider, model, feature, status).inc();
  for (const [kind, v] of Object.entries(normalizeUsage(usage))) if (v) LLM_TOKENS.labels(provider, model, feature, kind).inc(v);
  const cost = costUsd ?? usage?.cost;
  if (cost) LLM_COST.labels(provider, model, feature).inc(n(cost));
  if (duration !== undefined) LLM_DURATION.labels(provider, model, feature).observe(duration);
}

/** Start an AI call; set .usage/.costUsd/.model, call .fail(err, status) on errors, always .end(). */
export function llmCall(provider, model, feature) {
  const start = process.hrtime.bigint();
  const call = {
    provider, model, feature, usage: undefined, costUsd: undefined, status: "ok",
    /** The user stopped it or the client left: end() records nothing. */
    cancel() { call.status = null; },
    fail(err, httpStatus) {
      if (err?.name === "AbortError" && !httpStatus) { call.status = null; return; }
      call.status = classifyError(err, httpStatus);
      log("error", "AI call failed", { event: "llm_error", provider, model: call.model, feature, status: call.status, err }, "llm");
    },
    end() {
      if (call.status === null) return;
      const duration = Number(process.hrtime.bigint() - start) / 1e9;
      recordLlm(call.provider, call.model, call.feature, call.usage, { status: call.status, duration, costUsd: call.costUsd });
    },
  };
  return call;
}

// ── Use, sign-ins, jobs, data ────────────────────────────────────────────────

export const feature = (name, by = 1) => FEATURES.labels(name).inc(by);

const seen = new Map();
const WINDOWS = { "1h": 3600, "24h": 86400, "7d": 604800 };
new client.Gauge({
  name: "neo_app_active_users",
  help: "Different users seen in the window.",
  labelNames: ["window"],
  collect() {
    const now = Date.now() / 1000;
    for (const [u, t] of seen) if (t < now - WINDOWS["7d"]) seen.delete(u);
    for (const [w, s] of Object.entries(WINDOWS)) this.labels(w).set([...seen.values()].filter((t) => t >= now - s).length);
  },
});

/** Note a request by a signed-in user (counts restart from zero on restart). */
export const seenUser = (user) => { if (user) seen.set(user, Date.now() / 1000); };

export function login(method, ok, user, reason) {
  const result = ok ? "success" : "failure";
  LOGINS.labels(method, result).inc();
  log(ok ? "info" : "warning", `sign-in ${ok ? "ok" : "failed"}`, { event: "login", method, result, user, reason }, "auth");
  if (ok) seenUser(user);
}

/** Run a background job: counted, timed, last run and last success, failures logged and rethrown. */
export async function job(name, fn) {
  const end = JOB_DURATION.labels(name).startTimer();
  try {
    const r = await fn();
    JOB_RUNS.labels(name, "success").inc();
    JOB_LAST_RUN.labels(name).set(Date.now() / 1000);
    JOB_LAST_SUCCESS.labels(name).set(Date.now() / 1000);
    log("info", `job ${name} done`, { event: "job_done", job: name }, "jobs");
    return r;
  } catch (err) {
    JOB_RUNS.labels(name, "failure").inc();
    JOB_LAST_RUN.labels(name).set(Date.now() / 1000);
    log("error", `job ${name} failed`, { event: "job_failed", job: name, err }, "jobs");
    throw err;
  } finally {
    end();
  }
}

export const dataUpdated = (source, ts = Date.now() / 1000) => DATA_UPDATED.labels(source).set(ts);
