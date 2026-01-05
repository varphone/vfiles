import axios, { AxiosInstance, AxiosError } from "axios";
import type { ApiResponse } from "../types";

class ApiService {
  private api: AxiosInstance;

  constructor() {
    this.api = axios.create({
      baseURL: "/api",
      timeout: 30000,
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
        const message = this.handleError(error);
        return Promise.reject(new Error(message));
      },
    );
  }

  private handleError(error: AxiosError): string {
    if (error.response) {
      const data = error.response.data as ApiResponse;
      return data.error || "请求失败";
    } else if (error.request) {
      return "网络错误，请检查连接";
    } else {
      return error.message || "未知错误";
    }
  }

  get<T = any>(url: string, params?: any): Promise<ApiResponse<T>> {
    return this.api.get(url, { params });
  }

  post<T = any>(url: string, data?: any): Promise<ApiResponse<T>> {
    return this.api.post(url, data);
  }

  put<T = any>(url: string, data?: any): Promise<ApiResponse<T>> {
    return this.api.put(url, data);
  }

  delete<T = any>(url: string, params?: any): Promise<ApiResponse<T>> {
    return this.api.delete(url, { params });
  }

  postForm<T = any>(url: string, formData: FormData): Promise<ApiResponse<T>> {
    return this.api.post(url, formData, {
      headers: {
        "Content-Type": "multipart/form-data",
      },
    });
  }

  postFormWithProgress<T = any>(
    url: string,
    formData: FormData,
    opts?: {
      signal?: AbortSignal;
      onUploadProgress?: (p: { loaded: number; total?: number }) => void;
    },
  ): Promise<ApiResponse<T>> {
    return this.api.post(url, formData, {
      headers: {
        "Content-Type": "multipart/form-data",
      },
      signal: opts?.signal,
      onUploadProgress: opts?.onUploadProgress
        ? (evt) => {
            const loaded = evt.loaded ?? 0;
            const total = evt.total ?? undefined;
            opts.onUploadProgress?.({ loaded, total });
          }
        : undefined,
    });
  }

  postBinaryWithProgress<T = any>(
    url: string,
    data: ArrayBuffer,
    opts?: {
      params?: Record<string, string | number | boolean | undefined>;
      signal?: AbortSignal;
      onUploadProgress?: (p: { loaded: number; total?: number }) => void;
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
