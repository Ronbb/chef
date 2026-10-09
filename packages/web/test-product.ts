import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

export function productWebUrl(path: string) {
  if (!process.env.CHEF_PRODUCT_WEB)
    throw new Error("Missing test product web root");
  return pathToFileURL(resolve(process.env.CHEF_PRODUCT_WEB, path));
}
export function browserCliUrl() {
  if (!process.env.CHEF_BROWSER_CLI)
    throw new Error("Missing browser test CLI path");
  return pathToFileURL(process.env.CHEF_BROWSER_CLI);
}
