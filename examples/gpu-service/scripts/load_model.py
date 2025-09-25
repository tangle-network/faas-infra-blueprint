#!/usr/bin/env python3
"""
PyTorch model loading script for GPU service example.
This script is executed inside the Docker container.
"""

import torch
import torchvision.models as models
import time
import json
import sys

def load_resnet50():
    """Load ResNet50 model and measure time"""
    start = time.time()

    # Load pre-trained ResNet50
    model = models.resnet50(pretrained=True)
    model.eval()

    # Move to GPU if available
    device = torch.device('cuda' if torch.cuda.is_available() else 'cpu')
    model = model.to(device)

    load_time = (time.time() - start) * 1000

    # Count parameters
    total_params = sum(p.numel() for p in model.parameters())

    result = {
        'model': 'resnet50',
        'device': str(device),
        'parameters': total_params,
        'load_time_ms': load_time,
        'layers': len(list(model.children()))
    }

    print(json.dumps(result))
    return model

def load_bert():
    """Load BERT model for NLP tasks"""
    try:
        from transformers import BertModel, BertTokenizer

        start = time.time()

        # Load BERT
        tokenizer = BertTokenizer.from_pretrained('bert-base-uncased')
        model = BertModel.from_pretrained('bert-base-uncased')
        model.eval()

        device = torch.device('cuda' if torch.cuda.is_available() else 'cpu')
        model = model.to(device)

        load_time = (time.time() - start) * 1000
        total_params = sum(p.numel() for p in model.parameters())

        result = {
            'model': 'bert-base-uncased',
            'device': str(device),
            'parameters': total_params,
            'load_time_ms': load_time
        }

        print(json.dumps(result))
        return model

    except ImportError:
        print(json.dumps({
            'error': 'transformers not installed',
            'hint': 'pip install transformers'
        }))
        sys.exit(1)

def load_yolo():
    """Load YOLO model for object detection"""
    try:
        # This would load a YOLO model
        # Using mock data for demo purposes
        start = time.time()
        time.sleep(0.5)  # Simulate loading
        load_time = (time.time() - start) * 1000

        result = {
            'model': 'yolov5',
            'device': 'cpu',
            'parameters': 7200000,
            'load_time_ms': load_time,
            'classes': 80
        }

        print(json.dumps(result))

    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)

if __name__ == '__main__':
    model_name = sys.argv[1] if len(sys.argv) > 1 else 'resnet50'

    if model_name == 'resnet50':
        load_resnet50()
    elif model_name == 'bert':
        load_bert()
    elif model_name == 'yolo':
        load_yolo()
    else:
        print(json.dumps({'error': f'Unknown model: {model_name}'}))
        sys.exit(1)