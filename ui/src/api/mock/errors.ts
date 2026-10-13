import type { ApiError, ApiErrorCode } from "../types";

export const apiError = (code: ApiErrorCode, message: string): ApiError => ({ code, message });
