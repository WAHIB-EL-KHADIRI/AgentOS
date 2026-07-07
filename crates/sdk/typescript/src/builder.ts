import type { AgentInfo, Tool } from "./types.js";
import { AgentClient } from "./client.js";

export class AgentBuilder {
  private name: string;
  private capabilities: string[] = [];
  private tools: Tool[] = [];

  constructor(name: string) {
    this.name = name;
  }

  withCapability(cap: string): this {
    this.capabilities.push(cap);
    return this;
  }

  withCapabilities(caps: string[]): this {
    this.capabilities.push(...caps);
    return this;
  }

  withTool(tool: Tool): this {
    this.tools.push(tool);
    return this;
  }

  async spawn(client: AgentClient): Promise<AgentInfo> {
    return client.spawnAgent(this.name, this.capabilities);
  }
}
