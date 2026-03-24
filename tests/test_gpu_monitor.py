"""Tests for GPU Monitor"""

import pytest
from nvidia_smi_manager.core.gpu_monitor import GPUMonitor, GPUInfo


class TestGPUMonitor:
    """Test GPU monitoring functionality"""

    def test_gpu_monitor_init(self):
        """Test GPUMonitor initialization"""
        monitor = GPUMonitor()
        assert monitor is not None
        assert isinstance(monitor.gpus, list)

    def test_get_all_gpus(self):
        """Test getting all GPUs"""
        monitor = GPUMonitor()
        gpus = monitor.get_all_gpus()
        assert isinstance(gpus, list)

    def test_gpu_info_dataclass(self):
        """Test GPUInfo dataclass"""
        gpu_info = GPUInfo(
            index=0,
            name="Test GPU",
            memory_total=8000,
            memory_used=4000,
            memory_free=4000,
            temperature=60.0,
            power_draw=150.0,
            power_limit=250.0,
            utilization=75.0
        )
        
        assert gpu_info.index == 0
        assert gpu_info.name == "Test GPU"
        assert gpu_info.memory_total == 8000
        assert gpu_info.temperature == 60.0


if __name__ == '__main__':
    pytest.main([__file__])
