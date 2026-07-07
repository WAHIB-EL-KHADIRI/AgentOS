"""
AutoGen + AgentOS Integration Example
======================================

Demonstrates how AutoGen agents can publish events to the AgentOS bus for
observability and replay.

Requirements:
    pip install pyautogen httpx

Run:
    export OPENAI_API_KEY="sk-..."
    python autogen_agents.py
"""

import json
import os
import uuid
from datetime import datetime
from typing import Any, Dict, Optional

try:
    import httpx
except ImportError:
    httpx = None  # type: ignore

try:
    import autogen
except ImportError:
    msg = "pyautogen is required. Install: pip install pyautogen"
    raise ImportError(msg)


class AgentOSEventLogger(autogen.runtime_logging.LoggingSession):
    """Logs AutoGen events to the AgentOS bus for observability."""

    def __init__(self, bus_url: str = "http://127.0.0.1:9876"):
        super().__init__()
        self.session_id = str(uuid.uuid4())
        self._http = httpx.Client(base_url=bus_url, timeout=5.0) if httpx else None

    def _publish(self, event_type: str, data: Dict[str, Any]) -> None:
        if not self._http:
            return
        try:
            payload = {
                "event_type": f"autogen.{event_type}",
                "session_id": self.session_id,
                "timestamp": datetime.utcnow().isoformat(),
                "data": data,
            }
            self._http.post("/api/events", json=payload)
        except Exception:
            pass

    def on_agent_start(self, agent_name: str, **kwargs) -> None:
        self._publish("agent.start", {"agent_name": agent_name, **kwargs})

    def on_agent_end(self, agent_name: str, **kwargs) -> None:
        self._publish("agent.end", {"agent_name": agent_name, **kwargs})

    def on_tool_call(self, agent_name: str, tool_name: str, arguments: str, result: str) -> None:
        self._publish("tool.call", {
            "agent_name": agent_name,
            "tool_name": tool_name,
            "arguments": arguments,
            "result": result,
        })

    def on_message(self, sender: str, receiver: str, message: Any) -> None:
        self._publish("message", {
            "from": sender,
            "to": receiver,
            "content": str(message)[:500],
        })

    def close(self) -> None:
        if self._http:
            self._http.close()


def main():
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("⚠  Set OPENAI_API_KEY environment variable for full functionality")
        print("   Running with mock configuration...\n")

    llm_config = {
        "config_list": [{"model": "gpt-4", "api_key": api_key or "mock"}],
        "temperature": 0.7,
    }

    # Set up AgentOS event logger
    logger = AgentOSEventLogger(bus_url=os.environ.get("AGENTOS_BUS_URL", "http://127.0.0.1:9876"))
    autogen.runtime_logging.start(logger=logger)

    print(f"{'='*60}")
    print("  AutoGen + AgentOS Integration Demo")
    print(f"{'='*60}")

    # Create AutoGen agents
    researcher = autogen.AssistantAgent(
        name="Researcher",
        llm_config=llm_config,
        system_message="You are a researcher. Find relevant information.",
    )

    analyst = autogen.AssistantAgent(
        name="Analyst",
        llm_config=llm_config,
        system_message="You are an analyst. Analyze data and draw conclusions.",
    )

    writer = autogen.AssistantAgent(
        name="Writer",
        llm_config=llm_config,
        system_message="You are a writer. Produce clear reports.",
    )

    user_proxy = autogen.UserProxyAgent(
        name="UserProxy",
        human_input_mode="NEVER",
        max_consecutive_auto_reply=3,
        code_execution_config=False,
    )

    # Start a group chat
    group_chat = autogen.GroupChat(
        agents=[user_proxy, researcher, analyst, writer],
        messages=[],
        max_round=6,
    )

    manager = autogen.GroupChatManager(
        groupchat=group_chat,
        llm_config=llm_config,
    )

    task = (
        "Research the pros and cons of Rust vs Python for building "
        "microservices. Provide a summary with your recommendation."
    )

    print(f"\n  Task: {task}")
    print(f"\n  Agents: Researcher → Analyst → Writer")
    print(f"\n  Starting conversation...\n")

    try:
        user_proxy.initiate_chat(
            manager,
            message=task,
        )
    except Exception as e:
        print(f"\n  ⚠  Error during agent chat: {e}")
        print("     (This is expected without a valid API key)")

    print(f"\n{'-'*60}")
    print("  Conversation recorded to AgentOS bus.")
    print("  Replay with: agentOS trace --session <session_id>")
    print(f"{'='*60}")

    autogen.runtime_logging.stop()
    logger.close()


if __name__ == "__main__":
    main()
