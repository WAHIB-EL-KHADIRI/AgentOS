// AgentOS TypeScript SDK Example
// Run with: npx tsx examples/typescript/example.ts

import { AgentClient, AgentBuilder } from "../../crates/sdk/typescript/src/index.js";

async function main() {
  const client = new AgentClient({
    baseUrl: "http://127.0.0.1:9876",
  });

  // Build and spawn an agent.
  const agent = await new AgentBuilder("research-ts-1")
    .withCapability("web_search")
    .withCapability("memory")
    .spawn(client);

  console.log("Agent spawned:", agent);

  // Publish a message.
  const msgId = await client.publish("research.query", {
    action: "search",
    query: "TypeScript agent frameworks 2026",
  });
  console.log("Published message:", msgId);

  // List all agents.
  const agents = await client.listAgents();
  console.log("Active agents:", agents.length);
}

main().catch(console.error);
