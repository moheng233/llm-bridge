const BASE = '/api/v1';

export interface ModelCapabilities {
  name: string;
  maxInputTokens: number;
  maxOutputTokens: number;
  toolCalling: boolean;
  vision: boolean;
  thinking?: boolean;
  adaptiveThinking?: boolean;
}

export interface CatalogModel {
  modelName: string;
  capabilities: ModelCapabilities;
}

export interface Provider {
  providerName: string;
  providerType: 'openai' | 'anthropic' | 'gemini';
  baseUrl: string | null;
  keyringService: string;
  keyringAccount: string;
}

export interface ProviderModel {
  modelName: string;
  providerName: string;
  providerModelName: string;
  priority: number;
}

export interface CreateProviderRequest {
  providerName: string;
  providerType: 'openai' | 'anthropic' | 'gemini';
  baseUrl?: string;
  keyringService: string;
  keyringAccount: string;
}

export interface UpdateProviderRequest {
  providerType: 'openai' | 'anthropic' | 'gemini';
  baseUrl?: string;
  keyringService: string;
  keyringAccount: string;
}

export interface CreateBindingRequest {
  modelName: string;
  providerModelName: string;
  priority: number;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(body.error ?? res.statusText);
  }
  if (res.status === 204) return undefined as T;
  return res.json();
}

export const api = {
  listModels: () => request<CatalogModel[]>('/models'),
  listAvailableModels: () => request<CatalogModel[]>('/models/available'),

  listProviders: () => request<Provider[]>('/providers'),
  getProvider: (name: string) => request<Provider>(`/providers/${encodeURIComponent(name)}`),
  createProvider: (body: CreateProviderRequest) =>
    request<Provider>('/providers', { method: 'POST', body: JSON.stringify(body) }),
  updateProvider: (name: string, body: UpdateProviderRequest) =>
    request<Provider>(`/providers/${encodeURIComponent(name)}`, { method: 'PUT', body: JSON.stringify(body) }),
  deleteProvider: (name: string) =>
    request<void>(`/providers/${encodeURIComponent(name)}`, { method: 'DELETE' }),

  listBindings: (providerName: string) =>
    request<ProviderModel[]>(`/providers/${encodeURIComponent(providerName)}/models`),
  createBinding: (providerName: string, body: CreateBindingRequest) =>
    request<ProviderModel>(`/providers/${encodeURIComponent(providerName)}/models`, {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  deleteBinding: (providerName: string, modelName: string) =>
    request<void>(
      `/providers/${encodeURIComponent(providerName)}/models/${encodeURIComponent(modelName)}`,
      { method: 'DELETE' },
    ),
};
