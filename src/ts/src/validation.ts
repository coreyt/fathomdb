// TS-side FFI string guard (AC-068a / AC-068b).
//
// napi-rs converts JS strings to Rust `String` via NAPI's UTF-8 path,
// which silently replaces lone UTF-16 surrogates with U+FFFD; the
// Rust-side guard never sees them. We catch them in TS BEFORE the
// native call so the no-row-written invariant holds end-to-end.

import { WriteValidationError } from "./errors.js";

export function validateFfiString(value: string): void {
  for (let i = 0; i < value.length; i++) {
    const code = value.charCodeAt(i);
    if (code === 0) {
      throw new WriteValidationError("embedded NUL byte in FFI string");
    }
    if (code >= 0xd800 && code <= 0xdbff) {
      if (i + 1 >= value.length) {
        throw new WriteValidationError("unpaired UTF-16 high surrogate in FFI string");
      }
      const next = value.charCodeAt(i + 1);
      if (next < 0xdc00 || next > 0xdfff) {
        throw new WriteValidationError("unpaired UTF-16 high surrogate in FFI string");
      }
      i++;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      throw new WriteValidationError("unpaired UTF-16 low surrogate in FFI string");
    }
  }
}

export function validateFfiTree(value: unknown): void {
  if (typeof value === "string") {
    validateFfiString(value);
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) {
      validateFfiTree(item);
    }
    return;
  }
  if (value !== null && typeof value === "object") {
    for (const v of Object.values(value as Record<string, unknown>)) {
      validateFfiTree(v);
    }
  }
}
