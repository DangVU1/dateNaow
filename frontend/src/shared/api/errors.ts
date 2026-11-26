// The backend error envelope is `{"error":{"code","message","fields"}}`,
// where `fields` (422 only) maps a field name to its list of error codes.

export type FieldErrors = Record<string, string[]>;

const GENERIC_MESSAGE = 'Đã có lỗi, bạn thử lại sau nhé.';
const NETWORK_MESSAGE =
  'Không kết nối được máy chủ, bạn kiểm tra mạng rồi thử lại nhé.';

export interface ApiErrorOptions {
  fields?: FieldErrors;
  retryAfterSecs?: number;
  cause?: unknown;
}

export class ApiError extends Error {
  /** HTTP status, or 0 when the request never got a response. */
  readonly status: number;
  /** Code from the server, or `unknown_error` / `network_error`. */
  readonly code: string;
  readonly fields?: FieldErrors;
  readonly retryAfterSecs?: number;

  constructor(
    status: number,
    code: string,
    message: string = GENERIC_MESSAGE,
    options: ApiErrorOptions = {},
  ) {
    super(message, { cause: options.cause });
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    if (options.fields !== undefined) this.fields = options.fields;
    if (options.retryAfterSecs !== undefined) {
      this.retryAfterSecs = options.retryAfterSecs;
    }
  }
}

/** Wraps an exception thrown by `fetch` itself (offline, DNS, aborted). */
export function networkError(cause: unknown): ApiError {
  return new ApiError(0, 'network_error', NETWORK_MESSAGE, { cause });
}

export async function toApiError(res: Response): Promise<ApiError> {
  const retryAfterSecs = parseRetryAfter(res.headers.get('Retry-After'));
  let body: unknown;
  try {
    body = await res.json();
  } catch {
    body = undefined;
  }
  const envelope = readEnvelope(body);
  if (envelope === null) {
    return new ApiError(res.status, 'unknown_error', GENERIC_MESSAGE, {
      retryAfterSecs,
    });
  }
  return new ApiError(
    res.status,
    envelope.code,
    envelope.message ?? GENERIC_MESSAGE,
    { fields: envelope.fields, retryAfterSecs },
  );
}

/** Only the delta-seconds form; the backend never sends an HTTP date. */
function parseRetryAfter(value: string | null): number | undefined {
  if (value === null) return undefined;
  const trimmed = value.trim();
  return /^\d+$/.test(trimmed) ? Number(trimmed) : undefined;
}

interface Envelope {
  code: string;
  message?: string;
  fields?: FieldErrors;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readEnvelope(body: unknown): Envelope | null {
  if (!isRecord(body) || !isRecord(body.error)) return null;
  const { code, message, fields } = body.error;
  if (typeof code !== 'string') return null;
  return {
    code,
    message: typeof message === 'string' ? message : undefined,
    fields: readFields(fields),
  };
}

function readFields(value: unknown): FieldErrors | undefined {
  if (!isRecord(value)) return undefined;
  const out: FieldErrors = {};
  for (const [field, codes] of Object.entries(value)) {
    if (Array.isArray(codes)) {
      out[field] = codes.filter((c): c is string => typeof c === 'string');
    }
  }
  return out;
}
