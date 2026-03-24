"""Tests for GPU control functionality"""

import pytest
from pathlib import Path
from unittest.mock import Mock, patch, MagicMock
from nvidia_smi_manager.core.gpu_control import GPUController, GPUProfile
from nvidia_smi_manager.core.config import Config


class TestGPUProfile:
    """Test GPUProfile dataclass"""
    
    def test_create_profile(self):
        """Test creating a GPU profile"""
        profile = GPUProfile(
            gpu_index=0,
            core_clock_offset=50,
            memory_clock_offset=100,
            power_limit=250
        )
        
        assert profile.gpu_index == 0
        assert profile.core_clock_offset == 50
        assert profile.memory_clock_offset == 100
        assert profile.power_limit == 250
    
    def test_create_profile_with_defaults(self):
        """Test creating a profile with default values"""
        profile = GPUProfile(gpu_index=1)
        
        assert profile.gpu_index == 1
        assert profile.core_clock_offset == 0
        assert profile.memory_clock_offset == 0
        assert profile.power_limit is None


class TestGPUController:
    """Test GPUController class"""
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_check_nvidia_settings_success(self, mock_run):
        """Test successful nvidia-settings check"""
        mock_run.return_value = MagicMock(returncode=0)
        
        # Should not raise an error
        controller = GPUController()
        assert controller is not None
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_check_nvidia_settings_failure(self, mock_run):
        """Test failed nvidia-settings check"""
        mock_run.side_effect = FileNotFoundError()
        
        with pytest.raises(RuntimeError, match="nvidia-settings not found"):
            GPUController()
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_set_core_clock_offset_valid(self, mock_run):
        """Test setting valid core clock offset"""
        mock_run.return_value = MagicMock(returncode=0)
        
        controller = GPUController()
        result = controller.set_core_clock_offset(0, 50)
        
        assert result is True
        mock_run.assert_called()
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_set_core_clock_offset_out_of_bounds(self, mock_run):
        """Test setting out-of-bounds core clock offset"""
        mock_run.return_value = MagicMock(returncode=0)
        
        controller = GPUController()
        
        # Test lower bound
        with pytest.raises(ValueError, match="out of bounds"):
            controller.set_core_clock_offset(0, -100)
        
        # Test upper bound
        with pytest.raises(ValueError, match="out of bounds"):
            controller.set_core_clock_offset(0, 150)
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_set_core_clock_offset_skip_validation(self, mock_run):
        """Test setting offset while skipping validation"""
        mock_run.return_value = MagicMock(returncode=0)
        
        controller = GPUController()
        result = controller.set_core_clock_offset(0, -100, validate=False)
        
        # Should not raise error when validate=False
        assert result is True
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_set_memory_clock_offset_valid(self, mock_run):
        """Test setting valid memory clock offset"""
        mock_run.return_value = MagicMock(returncode=0)
        
        controller = GPUController()
        result = controller.set_memory_clock_offset(0, 75)
        
        assert result is True
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_set_power_limit_valid(self, mock_run):
        """Test setting valid power limit"""
        mock_run.return_value = MagicMock(returncode=0)
        
        controller = GPUController()
        result = controller.set_power_limit(0, 250)
        
        assert result is True
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_set_power_limit_out_of_bounds(self, mock_run):
        """Test setting out-of-bounds power limit"""
        mock_run.return_value = MagicMock(returncode=0)
        
        controller = GPUController()
        
        with pytest.raises(ValueError, match="out of bounds"):
            controller.set_power_limit(0, 1000)
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_apply_profile(self, mock_run):
        """Test applying a complete GPU profile"""
        mock_run.return_value = MagicMock(returncode=0)
        
        controller = GPUController()
        profile = GPUProfile(
            gpu_index=0,
            core_clock_offset=50,
            memory_clock_offset=100,
            power_limit=250
        )
        
        result = controller.apply_profile(profile)
        assert result is True
        assert mock_run.call_count > 0
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_reset_gpu(self, mock_run):
        """Test resetting GPU to default settings"""
        mock_run.return_value = MagicMock(returncode=0)
        
        controller = GPUController()
        result = controller.reset_gpu(0)
        
        assert result is True
        # Should call nvidia-settings at least twice (core and memory reset)
        assert mock_run.call_count >= 2
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_nvidia_settings_permission_denied(self, mock_run, capsys):
        """Test handling of permission denied errors"""
        mock_run.return_value = MagicMock(
            returncode=1,
            stderr="Permission denied"
        )
        
        controller = GPUController()
        result = controller.set_core_clock_offset(0, 50)
        
        assert result is False
        captured = capsys.readouterr()
        assert "Permission denied" in captured.out or "Permission denied" in captured.err
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_query_nvidia_settings(self, mock_run):
        """Test querying nvidia-settings"""
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout="Attribute 'GPUGraphicsClockOffset' (nvidia0:0): 50."
        )
        
        controller = GPUController()
        result = controller.get_core_clock_offset(0)
        
        assert result == 50
    
    @patch('nvidia_smi_manager.core.gpu_control.subprocess.run')
    def test_query_nvidia_settings_invalid(self, mock_run):
        """Test querying invalid nvidia-settings response"""
        mock_run.return_value = MagicMock(
            returncode=1,
            stdout=""
        )
        
        controller = GPUController()
        result = controller.get_core_clock_offset(0)
        
        assert result is None
    
    def test_custom_safety_limits(self):
        """Test initializing controller with custom safety limits"""
        custom_limits = {
            "core_clock_range": (-50, 150),
            "memory_clock_range": (-50, 150),
            "power_limit_range": (100, 400),
        }
        
        with patch('nvidia_smi_manager.core.gpu_control.subprocess.run'):
            controller = GPUController(safety_limits=custom_limits)
            assert controller.safety_limits == custom_limits


class TestConfig:
    """Test Config class for GPU profiles"""
    
    def test_set_gpu_profile(self):
        """Test setting a GPU profile"""
        config = Config()
        profile = {
            "core_clock_offset": 50,
            "memory_clock_offset": 100,
            "power_limit": 250
        }
        
        config.set_gpu_profile(0, profile)
        
        assert config.gpu_profiles["0"] == profile
    
    def test_get_gpu_profile(self):
        """Test getting a GPU profile"""
        config = Config()
        profile = {
            "core_clock_offset": 50,
            "memory_clock_offset": 100
        }
        
        config.set_gpu_profile(1, profile)
        retrieved = config.get_gpu_profile(1)
        
        assert retrieved == profile
    
    def test_get_nonexistent_gpu_profile(self):
        """Test getting a nonexistent GPU profile"""
        config = Config()
        result = config.get_gpu_profile(99)
        
        assert result is None
    
    def test_get_all_gpu_profiles(self):
        """Test getting all GPU profiles"""
        config = Config()
        profile1 = {"core_clock_offset": 50}
        profile2 = {"core_clock_offset": 100}
        
        config.set_gpu_profile(0, profile1)
        config.set_gpu_profile(1, profile2)
        
        profiles = config.get_all_gpu_profiles()
        assert profiles["0"] == profile1
        assert profiles["1"] == profile2
    
    def test_set_gpu_limits(self):
        """Test setting custom GPU limits"""
        config = Config()
        custom_limits = {
            "core_clock_range": (-100, 200),
        }
        
        config.set_gpu_limits(custom_limits)
        
        assert config.gpu_limits == custom_limits
    
    def test_save_and_load_config_with_profiles(self, tmp_path):
        """Test saving and loading config with GPU profiles"""
        config_file = tmp_path / "test_config.json"
        
        config = Config()
        config.set_gpu_profile(0, {"core_clock_offset": 50, "power_limit": 250})
        config.save_to_file(config_file)
        
        # Load config
        loaded_config = Config.from_file(config_file)
        
        assert loaded_config.get_gpu_profile(0) == {"core_clock_offset": 50, "power_limit": 250}
    
    def test_default_gpu_limits(self):
        """Test default GPU limits"""
        config = Config()
        
        assert "core_clock_range" in config.gpu_limits
        assert "memory_clock_range" in config.gpu_limits
        assert "power_limit_range" in config.gpu_limits
        
        # Check default values
        assert config.gpu_limits["core_clock_range"] == (-75, 100)
        assert config.gpu_limits["memory_clock_range"] == (-75, 100)
        assert config.gpu_limits["power_limit_range"] == (0, 500)
