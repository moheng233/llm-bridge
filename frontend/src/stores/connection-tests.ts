import { defineStore } from "pinia";
import { reactive } from "vue";
export interface ConnectionTestRecord {
  modelId: number;
  providerId: number;
  linkId: number;
  testedAt: number;
  status: "success" | "failure";
  latencyMs: number | null;
  message: string | null;
}
export const useConnectionTestsStore = defineStore("connection-tests", () => {
  const records = reactive(new Map<number, ConnectionTestRecord>());
  function set(record: ConnectionTestRecord) {
    records.set(record.linkId, record);
  }
  function get(linkId: number) {
    return records.get(linkId);
  }
  function invalidateProvider(id: number) {
    for (const [key, record] of records) if (record.providerId === id) records.delete(key);
  }
  function invalidateModel(id: number) {
    for (const [key, record] of records) if (record.modelId === id) records.delete(key);
  }
  function invalidateLink(id: number) {
    records.delete(id);
  }
  function clear() {
    records.clear();
  }
  return { set, get, invalidateProvider, invalidateModel, invalidateLink, clear };
});
