"""GPU monitoring core functionality"""

import subprocess
from dataclasses import dataclass
from typing import List, Optional


@dataclass
class GPUInfo:
    """Data class for GPU information"""
    index: int
    name: str
    memory_total: int  # MB
    memory_used: int   # MB
    memory_free: int   # MB
    temperature: Optional[float] = None
    power_draw: Optional[float] = None  # Watts
    power_limit: Optional[float] = None  # Watts
    utilization: Optional[float] = None  # Percentage
    core_clock_offset: Optional[int] = None  # MHz
    memory_clock_offset: Optional[int] = None  # MHz
    power_limit_offset: Optional[int] = None  # Watts


class GPUMonitor:
    """Monitor NVIDIA GPUs"""

    def __init__(self):
        """Initialize GPU monitor"""
        self.gpus: List[GPUInfo] = []
        self._update_gpu_info()

    def _update_gpu_info(self) -> None:
        """Update GPU information from nvidia-smi"""
        try:
            result = subprocess.run(
                [
                    "nvidia-smi",
                    "--query-gpu=index,name,memory.total,memory.used,memory.free,"
                    "temperature.gpu,power.draw,power.limit,utilization.gpu",
                    "--format=csv,noheader,nounits"
                ],
                capture_output=True,
                text=True,
                check=True
            )
            
            self.gpus = []
            for line in result.stdout.strip().split('\n'):
                if not line:
                    continue
                
                parts = [p.strip() for p in line.split(',')]
                try:
                    gpu_info = GPUInfo(
                        index=int(parts[0]),
                        name=parts[1],
                        memory_total=int(float(parts[2])),
                        memory_used=int(float(parts[3])),
                        memory_free=int(float(parts[4])),
                        temperature=float(parts[5]) if parts[5] != 'N/A' else None,
                        power_draw=float(parts[6]) if parts[6] != 'N/A' else None,
                        power_limit=float(parts[7]) if parts[7] != 'N/A' else None,
                        utilization=float(parts[8]) if parts[8] != 'N/A' else None,
                    )
                    self.gpus.append(gpu_info)
                except (ValueError, IndexError) as e:
                    print(f"Error parsing GPU info: {e}")
        except subprocess.CalledProcessError as e:
            print(f"Error running nvidia-smi: {e}")
        except FileNotFoundError:
            print("nvidia-smi not found. Please install NVIDIA drivers.")

    def get_gpu(self, index: int) -> Optional[GPUInfo]:
        """Get GPU info by index"""
        for gpu in self.gpus:
            if gpu.index == index:
                return gpu
        return None

    def get_all_gpus(self) -> List[GPUInfo]:
        """Get all GPUs"""
        self._update_gpu_info()
        return self.gpus

    def get_memory_usage(self) -> tuple:
        """Get total memory usage across all GPUs"""
        total_memory = sum(gpu.memory_total for gpu in self.gpus)
        used_memory = sum(gpu.memory_used for gpu in self.gpus)
        return used_memory, total_memory
