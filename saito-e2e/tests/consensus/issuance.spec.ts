import { test } from "@playwright/test";

let fs = require("fs");
let process = require("process");

test("issuance file generation @consensus", async ({ page }, testInfo) => {
  testInfo.setTimeout(0);
  let dir = "./temp";
  if (fs.existsSync(dir)) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir);
  }

  // generate some blocks
  process.exec();

  // run utxo file generation in SLR

  // check the entries in the generated issuance file
  console.log("cwd = " + process.cwd());
});
