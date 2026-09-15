import { invoke } from '@tauri-apps/api/core'
import type { ProviderAdapterId } from '../domain/settings'
import type { ModelCapabilities } from '../types'
import { adapterIdToLegacy } from './provider-service'

export async function getModelCapabilities(
  adapterId: ProviderAdapterId,
  model: string
): Promise<ModelCapabilities> {
  return invoke<ModelCapabilities>('model_capabilities', {
    adapter: adapterIdToLegacy(adapterId),
    model
  })
}
