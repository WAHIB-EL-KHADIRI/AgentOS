"""
LangGraph + AgentOS Integration Example
========================================

Demonstrates how to run a LangGraph agent workflow inside the AgentOS runtime
layer. AgentOS provides supervision, tracing, secrets, and messaging while
LangGraph handles the agent state graph and LLM orchestration.

Requirements:
    pip install langgraph langchain-openai httpx

Run:
    export OPENAI_API_KEY="sk-..."
    python langgraph_agent.py
"""

import json
import os
import uuid
from datetime import datetime
from typing import Annotated, Any, Dict, List, Literal, TypedDict

try:
    import httpx
except ImportError:
    httpx = None  # type: ignore

try:
    from langgraph.graph import END, StateGraph
    from langgraph.checkpoint import MemorySaver
except ImportError:
    msg = (
        "langgraph is required. Install it with: pip install langgraph"
    )
    raise ImportError(msg)

# ---------------------------------------------------------------------------
# AgentOS Python SDK (embedded for the example)
# ---------------------------------------------------------------------------

class AgentOSClient:
    """Minimal AgentOS client for publishing events and recording traces."""

    def __init__(self, base_url: str = "http://127.0.0.1:9876"):
        self.base_url = base_url
        self.session_id = str(uuid.uuid4())
        self._http = httpx.Client(base_url=base_url, timeout=5.0)

    def publish_event(self, event_type: str, data: Dict[str, Any]) -> None:
        """Publish an event to the AgentOS bus."""
        try:
            payload = {
                "event_type": event_type,
                "session_id": self.session_id,
                "timestamp": datetime.utcnow().isoformat(),
                "data": data,
            }
            self._http.post("/api/events", json=payload)
        except Exception:
            pass  # Silently fail if AgentOS is not running

    def record_trace(self, step: str, state: Dict[str, Any]) -> None:
        """Record a trace checkpoint compatible with agentOS replay."""
        self.publish_event("trace.step", {"step": step, "state": state})

    def close(self) -> None:
        self._http.close()


# ---------------------------------------------------------------------------
# LangGraph State + Tools
# ---------------------------------------------------------------------------

class AgentState(TypedDict):
    messages: Annotated[List[Dict[str, str]], "Conversation messages"]
    next_step: str
    research_results: List[str]


def search_tool(query: str) -> str:
    """Simulated web search tool."""
    results = {
        "rust vs python performance": (
            "Rust is generally 10-100x faster than Python for CPU-bound "
            "tasks due to zero-cost abstractions and no GC overhead."
        ),
        "best language for web development": (
            "JavaScript/TypeScript dominate frontend; Python (Django, FastAPI) "
            "and Go are strong for backend; Rust is rising for performance-critical services."
        ),
    }
    return results.get(query.lower(), f"No results found for: {query}")


def calculator_tool(expression: str) -> str:
    """Evaluate a mathematical expression."""
    try:
        # Safe eval with restricted globals
        result = eval(expression, {"__builtins__": {}}, {})
        return str(result)
    except Exception as e:
        return f"Error: {e}"


def run_tool(tool_name: str, args: str) -> str:
    """Dispatch to the appropriate tool."""
    if tool_name == "web_search":
        return search_tool(args)
    elif tool_name == "calculator":
        return calculator_tool(args)
    return f"Unknown tool: {tool_name}"


# ---------------------------------------------------------------------------
# LangGraph nodes
# ---------------------------------------------------------------------------

def think_node(state: AgentState) -> Dict[str, Any]:
    """Analyze the current state and decide next action."""
    last_msg = state["messages"][-1]["content"] if state["messages"] else ""
    
    # Simple routing logic (in production, this would be an LLM call)
    if any(q in last_msg.lower() for q in ["search", "find", "what is", "compare"]):
        return {"next_step": "search", **state}
    elif any(q in last_msg.lower() for q in ["calculate", "compute", "math", "+", "-", "*", "/"]):
        return {"next_step": "calculate", **state}
    else:
        return {"next_step": "respond", **state}

def tool_node(state: AgentState) -> Dict[str, Any]:
    """Execute the selected tool."""
    last_msg = state["messages"][-1]["content"] if state["messages"] else ""
    
    if state["next_step"] == "search":
        result = run_tool("web_search", last_msg)
        state.setdefault("research_results", []).append(result)
        return {"research_results": state["research_results"], "next_step": "respond"}
    elif state["next_step"] == "calculate":
        result = run_tool("calculator", last_msg)
        return {"research_results": state.get("research_results", []) + [result], "next_step": "respond"}
    return {"next_step": "respond"}

def respond_node(state: AgentState) -> Dict[str, Any]:
    """Generate a response based on results."""
    results = state.get("research_results", [])
    if results:
        response = f"Based on my research:\n" + "\n".join(f"  - {r}" for r in results)
    else:
        response = f"I received your message: {state['messages'][-1]['content']}"
    
    return {
        "messages": state["messages"] + [{"role": "assistant", "content": response}],
        "next_step": "end",
    }

def should_continue(state: AgentState) -> Literal["tools", "respond", "__end__"]:
    if state["next_step"] in ("search", "calculate"):
        return "tools"
    elif state["next_step"] == "respond":
        return "respond"
    return "__end__"


# ---------------------------------------------------------------------------
# Build the graph
# ---------------------------------------------------------------------------

def build_agent_graph() -> StateGraph:
    workflow = StateGraph(AgentState)

    workflow.add_node("think", think_node)
    workflow.add_node("tools", tool_node)
    workflow.add_node("respond", respond_node)

    workflow.set_entry_point("think")

    workflow.add_conditional_edges(
        "think",
        should_continue,
        {"tools": "tools", "respond": "respond", "__end__": END},
    )
    workflow.add_edge("tools", "respond")
    workflow.add_edge("respond", END)

    return workflow


# ---------------------------------------------------------------------------
# AgentOS integration wrapper
# ---------------------------------------------------------------------------

class AgentOSLangGraphRunner:
    """Wraps a LangGraph with AgentOS runtime services."""

    def __init__(
        self,
        agent_id: str,
        agent_name: str,
        bus_url: str = "http://127.0.0.1:9876",
    ):
        self.agent_id = agent_id
        self.agent_name = agent_name
        self.client = AgentOSClient(bus_url)

    def run(self, user_input: str) -> Dict[str, Any]:
        """Run a LangGraph agent workflow with AgentOS instrumentation."""
        print(f"\n{'='*60}")
        print(f"  Agent: {self.agent_name} ({self.agent_id})")
        print(f"  Input: {user_input}")
        print(f"{'='*60}")

        # Build and compile the graph
        graph = build_agent_graph()
        app = graph.compile(checkpointer=MemorySaver())

        # Initial state
        initial_state: AgentState = {
            "messages": [{"role": "user", "content": user_input}],
            "next_step": "think",
            "research_results": [],
        }

        config = {"configurable": {"thread_id": self.agent_id}}

        # Record initial trace
        self.client.record_trace("input", {"user_input": user_input})

        # Execute the graph step by step
        for event in app.stream(initial_state, config):
            for node_name, node_state in event.items():
                step_type = node_state.get("next_step", "unknown")
                self.client.record_trace(
                    f"node.{node_name}",
                    {"next_step": step_type, "results": node_state.get("research_results", [])},
                )
                self.client.publish_event("agent.step", {
                    "agent_id": self.agent_id,
                    "node": node_name,
                    "state": node_state,
                })
                print(f"\n  └─ Node: {node_name} → {step_type}")

        # Get final state
        final_state = app.get_state(config)
        final_messages = final_state.values.get("messages", [])

        self.client.record_trace("output", {"messages": final_messages})
        self.client.publish_event("agent.complete", {
            "agent_id": self.agent_id,
            "messages": final_messages,
        })

        print(f"\n{'─'*60}")
        print(f"  Final Response:")
        if final_messages:
            print(f"  {final_messages[-1]['content']}")
        print(f"{'='*60}\n")

        return final_state.values

    def close(self) -> None:
        self.client.close()


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print("🚀 LangGraph + AgentOS Integration Demo")
    print("   (AgentOS runtime is optional — runs standalone too)")
    print()

    # Create agent runner
    runner = AgentOSLangGraphRunner(
        agent_id="langgraph-agent-1",
        agent_name="Research Assistant",
        bus_url=os.environ.get("AGENTOS_BUS_URL", "http://127.0.0.1:9876"),
    )

    try:
        # Run 1: Web search
        runner.run("Compare Rust vs Python performance")

        # Run 2: Calculation
        runner.run("Calculate 42 * 17 + 256")

        # Run 3: General response
        runner.run("What is the best language for web development?")

    finally:
        runner.close()

    print("✅ Done. If AgentOS runtime was running, traces are recorded.")
    print("   Replay with: agentOS trace --id langgraph-agent-1")


if __name__ == "__main__":
    main()
