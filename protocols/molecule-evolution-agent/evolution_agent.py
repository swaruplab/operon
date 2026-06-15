import sys
import os
import random
from typing import Dict, Any

# Adjust path to find platform module
current_dir = os.path.dirname(os.path.abspath(__file__))
project_root = os.path.abspath(os.path.join(current_dir, "../../../.."))
platform_dir = os.path.join(project_root, "platform")

if platform_dir not in sys.path:
    sys.path.append(platform_dir)

from adapters.runtime_adapter import llm

class MoleculeEvolutionAgent:
    """
    Agent that iteratively designs molecules using LLM feedback.
    """
    def __init__(self):
        self.population = ["CC(=O)OC1=CC=CC=C1C(=O)O"] # Aspirin start
        self.generations = 3

    def evolve(self, target_protein: str) -> Dict[str, Any]:
        print(f"🧬 [Designer] Initiating evolution for target: {target_protein}")
        
        best_candidate = self.population[0]
        history = []
        
        for gen in range(self.generations):
            # Ask LLM to improve the molecule
            prompt = f"Evolve this SMILES '{best_candidate}' to better bind to {target_protein}. Suggest one modification."
            suggestion = llm.complete("You are a medicinal chemist.", prompt)
            
            # Extract SMILES from suggestion (Mock extraction)
            if "New SMILES:" in suggestion:
                new_smiles = suggestion.split("New SMILES:")[-1].strip()
                best_candidate = new_smiles
            else:
                # Fallback if LLM serves text only
                best_candidate += "F" 
            
            score = self._mock_docking_score(best_candidate)
            history.append(f"Gen {gen}: {best_candidate} (Score: {score:.2f})")
            
        return {
            "top_candidate": best_candidate,
            "score": score,
            "evolution_log": history,
            "rationale": suggestion
        }

    def _mock_docking_score(self, smiles: str) -> float:
        # Mock scoring function
        return 0.85 + (random.random() * 0.1)

if __name__ == "__main__":
    agent = MoleculeEvolutionAgent()
    print(agent.evolve("GPRC5D"))
