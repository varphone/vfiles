import axios, { AxiosInstance, AxiosError } from "axios";
import type { ApiResponse } from "../types";

type RequestOptions = {
  timeoutMs?: number;
  signal?: AbortSignal;
};

export class ApiError extends Error {
  status?: number;
  data?: ApiResponse;

  constructor(message: string, status?: number, data?: ApiResponse) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.data = data;
  }
}

class ApiService {
  private api: AxiosInstance;

  constructor() {
    this.api = axios.create({
      baseURL: "/api",
      timeout: 30000,
      withCredentials: true,
      headers: {
        "Content-Type": "application/json",
      },
    });

    // 请求拦截器
    this.api.interceptors.request.use(
      (config) => {
        return config;
      },
      (error) => {
        return Promise.reject(error);
      },
    );

    // 响应拦截器
    this.api.interceptors.response.use(
      (response) => {
        return response.data;
      },
      (error: AxiosError) => {
        const status = error.response?.status;
        if (status === 401 && typeof window !== "undefined") {
          window.dispatchEvent(new Event("vfiles:unauthorized"));
        }
        const message = this.handleError(error);
        return Promise.reject(
          new ApiError(
            message,
            status,
            error.response?.data as ApiResponse | undefined,
          ),
        );
      },
    );
  }

  private handleError(error: AxiosError): string {
    if (error.response) {
      const data = error.response.data as any;
      if (typeof data === "string" && data.trim()) {
        return data;
      }
      // Handle new Rust API error format
      if (data && data.message) {
        return data.message;
      }
      // Handle legacy API error format
      if (data && data.error) {
        return data.error;
      }
      return "请求失败";
    } else if (error.request) {
      return "网络错误，请检查连接";
    } else {
      return error.message || "未知错误";
    }
  }

  private dispatchUnauthorized(status?: number) {
    if (status === 401 && typeof window !== "undefined") {
      window.dispatchEvent(new Event("vfiles:unauthorized"));
    }
  }

  private extractErrorMessage(data: unknown): string {
    if (typeof data === "string" && data.trim()) {
      return data;
    }

    if (data && typeof data === "object") {
      const record = data as Record<string, unknown>;
      if (typeof record.message === "string" && record.message) {
        return record.message;
      }
      if (typeof record.error === "string" && record.error) {
        return record.error;
      }
    }

    return "请求失败";
  }

  private buildQueryString(
    params?: Record<string, string | number | boolean | undefined>,
  ): string {
    if (!params) return "";

    const search = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (value === undefined) continue;
      search.set(key, String(value));
    }

    const query = search.toString();
    return query ? `?${query}` : "";
  }

  private buildAxiosConfig(opts?: RequestOptions) {
    const config: { timeout?: number; signal?: AbortSignal } = {};
    if (opts?.timeoutMs !== undefined) {
      config.timeout = opts.timeoutMs;
    }
    if (opts?.signal) {
      config.signal = opts.signal;
    }
    return config;
  }

  get<T = any>(url: string, params?: any): Promise<ApiResponse<T>> {
    return this.api.get(url, { params });
  }

  post<T = any>(
    url: string,
    data?: any,
    opts?: RequestOptions,
  ): Promise<ApiResponse<T>> {
    return this.api.post(url, data, this.buildAxiosConfig(opts));
  }

  put<T = any>(
    url: string,
    data?: any,
    opts?: RequestOptions,
  ): Promise<ApiResponse<T>> {
    return this.api.put(url, data, this.buildAxiosConfig(opts));
  }

  delete<T = any>(url: string, params?: any): Promise<ApiResponse<T>> {
    return this.api.delete(url, { params });
  }

  postForm<T = any>(
    url: string,
    formData: FormData,
    opts?: RequestOptions,
  ): Promise<ApiResponse<T>> {
    return this.api.post(url, formData, {
      headers: {
        "Content-Type": "multipart/form-data",
      },
      ...this.buildAxiosConfig(opts),
    });
  }

  postFormWithProgress<T = any>(
    url: string,
    formData: FormData,
    opts?: {
      signal?: AbortSignal;
      onUploadProgress?: (p: { loaded: number; total?: number }) => void;
      timeoutMs?: number;
    },
  ): Promise<ApiResponse<T>> {
    return this.api.post(url, formData, {
      headers: {
        "Content-Type": "multipart/form-data",
      },
      signal: opts?.signal,
      timeout: opts?.timeoutMs,
      onUploadProgress: opts?.onUploadProgress
        ? (evt) => {
            const loaded = evt.loaded ?? 0;
            const total = evt.total ?? undefined;
            opts.onUploadProgress?.({ loaded, total });
          }
        : undefined,
    });
  }

  async putBinary<T = any>(
    url: string,
    data: ArrayBuffer,
    opts?: {
      params?: Record<string, string | number | boolean | undefined>;
      signal?: AbortSignal;
    },
  ): Promise<ApiResponse<T>> {
    const baseUrl = String(this.api.defaults.baseURL ?? "");
    const response = await fetch(
      `${baseUrl}${url}${this.buildQueryString(opts?.params)}`,
      {
        method: "PUT",
        body: data,
        signal: opts?.signal,
        credentials: "include",
        headers: {
          "Content-Type": "application/octet-stream",
        },
      },
    ).catch((error: unknown) => {
      if (error instanceof DOMException && error.name === "AbortError") {
        throw error;
      }
      throw new ApiError("网络错误，请检查连接");
    });

    const contentType = response.headers.get("content-type") || "";
    let payload: unknown;

    if (contentType.includes("application/json")) {
      payload = await response.json().catch(() => undefined);
    } else if (contentType.startsWith("text/")) {
      payload = await response.text().catch(() => "");
    }

    if (!response.ok) {
      this.dispatchUnauthorized(response.status);
      throw new ApiError(
        this.extractErrorMessage(payload),
        response.status,
        payload && typeof payload === "object"
          ? (payload as ApiResponse)
          : undefined,
      );
    }

    return payload as ApiResponse<T>;
  }

  async postFormNative<T = any>(
    url: string,
    formData: FormData,
    opts?: RequestOptions,
  ): Promise<ApiResponse<T>> {
    const baseUrl = String(this.api.defaults.baseURL ?? "");
    const response = await fetch(`${baseUrl}${url}`, {
      method: "POST",
      body: formData,
      signal: opts?.signal,
      credentials: "include",
    }).catch((error: unknown) => {
      if (error instanceof DOMException && error.name === "AbortError") {
        throw error;
      }
      throw new ApiError("网络错误，请检查连接");
    });

    const contentType = response.headers.get("content-type") || "";
    let payload: unknown;

    if (contentType.includes("application/json")) {
      payload = await response.json().catch(() => undefined);
    } else if (contentType.startsWith("text/")) {
      payload = await response.text().catch(() => "");
    }

    if (!response.ok) {
      this.dispatchUnauthorized(response.status);
      throw new ApiError(
        this.extractErrorMessage(payload),
        response.status,
        payload && typeof payload === "object"
          ? (payload as ApiResponse)
          : undefined,
      );
    }

    return payload as ApiResponse<T>;
  }

  postBinaryWithProgress<T = any>(
    url: string,
    data: ArrayBuffer,
    opts?: {
      params?: Record<string, string | number | boolean | undefined>;
      signal?: AbortSignal;
      onUploadProgress?: (p: { loaded: number; total?: number }) => void;
      timeoutMs?: number;
    },
  ): Promise<ApiResponse<T>> {
    const params: Record<string, string | number | boolean> | undefined =
      opts?.params
        ? Object.fromEntries(
            Object.entries(opts.params)
              .filter(([, v]) => v !== undefined)
              .map(([k, v]) => [k, v as any]),
          )
        : undefined;

    return this.api.post(url, data, {
      params,
      headers: {
        "Content-Type": "application/octet-stream",
      },
      signal: opts?.signal,
      timeout: opts?.timeoutMs,
      onUploadProgress: opts?.onUploadProgress
        ? (evt) => {
            const loaded = evt.loaded ?? 0;
            const total = evt.total ?? undefined;
            opts.onUploadProgress?.({ loaded, total });
          }
        : undefined,
    });
  }

  putBinaryWithProgress<T = any>(
    url: string,
    data: ArrayBuffer,
    opts?: {
      params?: Record<string, string | number | boolean | undefined>;
      signal?: AbortSignal;
      onUploadProgress?: (p: { loaded: number; total?: number }) => void;
      timeoutMs?: number;
    },
  ): Promise<ApiResponse<T>> {
    const params: Record<string, string | number | boolean> | undefined =
      opts?.params
        ? Object.fromEntries(
            Object.entries(opts.params)
              .filter(([, v]) => v !== undefined)
              .map(([k, v]) => [k, v as any]),
          )
        : undefined;

    return this.api.put(url, data, {
      params,
      headers: {
        "Content-Type": "application/octet-stream",
      },
      signal: opts?.signal,
      timeout: opts?.timeoutMs,
      onUploadProgress: opts?.onUploadProgress
        ? (evt) => {
            const loaded = evt.loaded ?? 0;
            const total = evt.total ?? undefined;
            opts.onUploadProgress?.({ loaded, total });
          }
        : undefined,
    });
  }
}

export const apiService = new ApiService();
