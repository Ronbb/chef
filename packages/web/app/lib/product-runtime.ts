import product from "@chef/product";
import { checkedNamespace } from "./product-session";

// Fixed build configuration shared by SSR, identity notices and private drafts.
export const productNamespace = checkedNamespace(product.sessionNamespace);
if (
  (product.id === "hargow" || product.id === "brioche") &&
  product.id !== productNamespace
)
  throw Error("Product session namespace does not match product");
