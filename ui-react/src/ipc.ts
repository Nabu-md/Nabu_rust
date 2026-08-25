import { invoke } from "@tauri-apps/api/core";

export const checkVaultExists = () => invoke<string | null>("check_vault_exists", {});
