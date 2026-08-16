// Example: Using the DiffFusion facade (Tier-0, detection-only API).
//
// Mirrors `examples/facade_usage.rs` in the Rust crate. Shows how library
// users interact with diff-fusion without needing to understand the internal
// implementation: load a schema, transform data from multiple source formats
// into the Common Intermediate Format (CIF), compare, and report.
//
// Run from the `ts/` directory:
//
//   npx tsx examples/facadeUsage.ts
//
// This example imports from `src/index.ts` directly rather than the built
// `dist/` output, so it works without a prior `npm run build`.

import { DiffFusion } from "../src/index.js";
import type { JsonValue } from "../src/domain/types.js";

// ---------------------------------------------------------------------------
// Intro
// ---------------------------------------------------------------------------

console.log("🎯 diff-fusion Facade API Examples\n");
console.log("═══════════════════════════════════════════════════════════\n");

// ---------------------------------------------------------------------------
// Step 1: Create DiffFusion with your schema
// ---------------------------------------------------------------------------
console.log("📋 Step 1: Initialize with schema\n");

const schema: JsonValue = {
  cif_schema: {
    product_id: { type: "string", required: true },
    product_name: { type: "string", required: true },
    price: { type: "number", required: true },
    in_stock: { type: "boolean", required: false },
  },
  transformations: {
    // Salesforce format
    salesforce: {
      product_id: { source_path: "Id", type: "string" },
      product_name: { source_path: "Name", type: "string" },
      price: { source_path: "Price__c", type: "number" },
      in_stock: { source_path: "Stock_Available__c", type: "boolean" },
    },
    // Shopify format
    shopify: {
      product_id: { source_path: "id", type: "string" },
      product_name: { source_path: "title", type: "string" },
      price: { source_path: "variants.0.price", type: "number" },
      in_stock: { source_path: "available", type: "boolean" },
    },
  },
};

const diffFusion = new DiffFusion(schema);
console.log("✅ DiffFusion initialized\n");

// ---------------------------------------------------------------------------
// Step 2: Transform data from different sources
// ---------------------------------------------------------------------------
console.log("🔄 Step 2: Transform data to CIF\n");

const salesforceData: JsonValue = {
  Id: "SF-001",
  Name: "Wireless Mouse",
  Price__c: 29.99,
  Stock_Available__c: true,
};

const salesforceResult = diffFusion.transform(salesforceData, "salesforce");
if (!salesforceResult.ok) {
  console.error(`❌ Salesforce transform failed: ${salesforceResult.error}`);
  process.exit(1);
}
const cifFromSalesforce = salesforceResult.value;
console.log("📤 Salesforce → CIF:");
console.log(`${JSON.stringify(cifFromSalesforce, null, 2)}\n`);

const shopifyData: JsonValue = {
  id: "SH-001",
  title: "Wireless Mouse",
  variants: [{ price: 34.99 }],
  available: true,
};

const shopifyResult = diffFusion.transform(shopifyData, "shopify");
if (!shopifyResult.ok) {
  console.error(`❌ Shopify transform failed: ${shopifyResult.error}`);
  process.exit(1);
}
const cifFromShopify = shopifyResult.value;
console.log("📤 Shopify → CIF:");
console.log(`${JSON.stringify(cifFromShopify, null, 2)}\n`);

// ---------------------------------------------------------------------------
// Step 3: Compare and detect conflicts
// ---------------------------------------------------------------------------
console.log("⚖️  Step 3: Detect conflicts between sources\n");

const report = diffFusion.compare(cifFromSalesforce, cifFromShopify);

if (report.hasConflicts) {
  console.log(`⚠️  Conflicts detected! Total: ${report.totalConflicts}\n`);
  for (const conflict of report.conflicts) {
    console.log(`  • Field '${conflict.path}' differs:`);
    console.log(`    Salesforce: ${conflict.oldValue}`);
    console.log(`    Shopify:    ${conflict.newValue}`);
  }
} else {
  console.log("✅ No conflicts - data is in sync!");
}
console.log();

// ---------------------------------------------------------------------------
// Step 4: One-liner workflow
// ---------------------------------------------------------------------------
console.log("⚡ Step 4: Transform and compare in one call\n");

const quickResult = diffFusion.transformAndCompare(
  salesforceData,
  "salesforce",
  shopifyData,
  "shopify",
);
if (!quickResult.ok) {
  console.error(`❌ Workflow failed: ${quickResult.error}`);
  process.exit(1);
}
const quickReport = quickResult.report;
console.log("📊 Quick Report:");
console.log(`   Conflicts: ${quickReport.totalConflicts}`);
console.log(`   Has conflicts: ${quickReport.hasConflicts}\n`);

// ---------------------------------------------------------------------------
// Step 5: Validation
// ---------------------------------------------------------------------------
console.log("✓ Step 5: Validate CIF data\n");

const salesforceValidation = diffFusion.validateCif(cifFromSalesforce);
if (salesforceValidation.ok) {
  console.log("✅ Salesforce CIF is valid");
} else {
  console.log("❌ Validation errors:");
  for (const err of salesforceValidation.errors) {
    console.log(`   - ${err}`);
  }
}

const shopifyValidation = diffFusion.validateCif(cifFromShopify);
if (shopifyValidation.ok) {
  console.log("✅ Shopify CIF is valid");
} else {
  console.log("❌ Validation errors:");
  for (const err of shopifyValidation.errors) {
    console.log(`   - ${err}`);
  }
}

// ---------------------------------------------------------------------------
// Outro
// ---------------------------------------------------------------------------
console.log("\n═══════════════════════════════════════════════════════════");
console.log("\n💡 Key Benefits:");
console.log("   • Simple API - no need to understand internals");
console.log("   • Type-safe - schema-driven transformations");
console.log("   • One instance handles all formats");
console.log("   • Built-in validation");
console.log("   • Clear conflict detection");
