"""
CrewAI + AgentOS Integration Example
=====================================

Demonstrates how a CrewAI crew can publish execution events to the AgentOS
bus for observability and trace recording.

Requirements:
    pip install crewai httpx

Run:
    export OPENAI_API_KEY="sk-..."
    python crewai_crew.py
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict

try:
    import httpx
except ImportError:
    httpx = None  # type: ignore

try:
    from crewai import Agent, Task, Crew, Process
except ImportError:
    msg = "crewai is required. Install: pip install crewai"
    raise ImportError(msg)


class AgentOSCrewCallback:
    """CrewAI callback that publishes events to the AgentOS bus."""

    def __init__(self, bus_url: str = "http://127.0.0.1:9876"):
        self.session_id = str(uuid.uuid4())
        self._http = httpx.Client(base_url=bus_url, timeout=5.0) if httpx else None

    def _publish(self, event_type: str, data: Dict[str, Any]) -> None:
        if not self._http:
            return
        try:
            payload = {
                "event_type": f"crewai.{event_type}",
                "session_id": self.session_id,
                "timestamp": datetime.utcnow().isoformat(),
                "data": data,
            }
            self._http.post("/api/events", json=payload)
        except Exception:
            pass

    def on_crew_start(self, crew, task: "Task") -> None:
        self._publish("task.start", {
            "crew": getattr(crew, "name", "unknown"),
            "task": task.description[:200] if task.description else "unknown",
        })

    def on_crew_end(self, crew, task: "Task", output: str) -> None:
        self._publish("task.end", {
            "crew": getattr(crew, "name", "unknown"),
            "task": task.description[:200] if task.description else "unknown",
            "output": output[:500],
        })

    def on_agent_action(self, agent_name: str, action: str, thought: str) -> None:
        self._publish("agent.action", {
            "agent": agent_name,
            "action": action,
            "thought": thought[:300],
        })

    def close(self) -> None:
        if self._http:
            self._http.close()


def main():
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("⚠  Set OPENAI_API_KEY environment variable for full functionality")
        print("   Running with mock configuration...\n")

    os.environ.setdefault("OPENAI_API_KEY", api_key or "mock")
    os.environ.setdefault("OPENAI_MODEL_NAME", "gpt-4")

    callback = AgentOSCrewCallback(
        bus_url=os.environ.get("AGENTOS_BUS_URL", "http://127.0.0.1:9876")
    )

    print(f"{'='*60}")
    print("  CrewAI + AgentOS Integration Demo")
    print(f"{'='*60}")

    # Define CrewAI agents
    researcher = Agent(
        role="Senior Researcher",
        goal="Find and summarize relevant information about the topic",
        backstory="You are an expert researcher with decades of experience.",
        allow_delegation=False,
        verbose=True,
    )

    writer = Agent(
        role="Technical Writer",
        goal="Create clear, concise documentation from research findings",
        backstory="You are a skilled technical writer who produces excellent documentation.",
        allow_delegation=False,
        verbose=True,
    )

    # Define tasks
    research_task = Task(
        description=(
            "Research the current state of AI agent frameworks in 2026. "
            "Focus on LangGraph, AutoGen, CrewAI, and custom agent runtimes. "
            "Identify key trends and pain points."
        ),
        expected_output="A comprehensive research summary with key findings",
        agent=researcher,
    )

    write_task = Task(
        description=(
            "Using the research provided, write a brief technical report "
            "about the AI agent framework landscape in 2026."
        ),
        expected_output="A well-structured markdown report",
        agent=writer,
    )

    # Create the crew
    crew = Crew(
        agents=[researcher, writer],
        tasks=[research_task, write_task],
        process=Process.sequential,
        verbose=True,
        name="ResearchCrew",
    )

    print("\n  Crew: ResearchCrew")
    print("  Agents: Senior Researcher → Technical Writer")
    print("  Process: Sequential")
    print("\n  Starting crew execution...\n")

    try:
        result = crew.kickoff()
        print(f"\n{'─'*60}")
        print("  Crew Output:")
        print(f"  {result}")
    except Exception as e:
        print(f"\n  ⚠  Error during crew execution: {e}")
        print("     (This is expected without a valid API key)")
        result = None

    print(f"\n{'─'*60}")
    print("  Execution recorded to AgentOS bus.")
    print("  Session ID: {callback.session_id}")
    print(f"{'='*60}")

    callback.close()


if __name__ == "__main__":
    main()
