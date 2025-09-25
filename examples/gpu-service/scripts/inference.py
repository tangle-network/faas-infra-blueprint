#!/usr/bin/env python3
"""
Inference script for GPU service example.
Performs actual inference on loaded models.
"""

import torch
import torchvision.transforms as transforms
import numpy as np
import json
import time
import sys
from PIL import Image
import io
import base64

def run_resnet_inference(image_data=None):
    """Run inference on ResNet50 model"""
    import torchvision.models as models

    # Load model (assumes already loaded in container)
    model = models.resnet50(pretrained=True)
    model.eval()

    # Prepare image
    transform = transforms.Compose([
        transforms.Resize(256),
        transforms.CenterCrop(224),
        transforms.ToTensor(),
        transforms.Normalize(mean=[0.485, 0.456, 0.406],
                           std=[0.229, 0.224, 0.225])
    ])

    # Create dummy image if none provided
    if image_data is None:
        dummy_image = Image.new('RGB', (224, 224), color='red')
    else:
        image_bytes = base64.b64decode(image_data)
        dummy_image = Image.open(io.BytesIO(image_bytes))

    input_tensor = transform(dummy_image).unsqueeze(0)

    # Run inference
    start = time.time()
    with torch.no_grad():
        output = model(input_tensor)
        probabilities = torch.nn.functional.softmax(output[0], dim=0)

    inference_time = (time.time() - start) * 1000

    # Get top 5 predictions
    top5_prob, top5_catid = torch.topk(probabilities, 5)

    result = {
        'model': 'resnet50',
        'inference_time_ms': inference_time,
        'top_predictions': [
            {
                'class_id': int(catid),
                'probability': float(prob)
            }
            for catid, prob in zip(top5_catid, top5_prob)
        ]
    }

    print(json.dumps(result))

def run_bert_inference(text="Hello, world!"):
    """Run inference on BERT model"""
    try:
        from transformers import BertModel, BertTokenizer

        # Load model and tokenizer
        tokenizer = BertTokenizer.from_pretrained('bert-base-uncased')
        model = BertModel.from_pretrained('bert-base-uncased')
        model.eval()

        # Tokenize input
        inputs = tokenizer(text, return_tensors='pt',
                          padding=True, truncation=True, max_length=512)

        # Run inference
        start = time.time()
        with torch.no_grad():
            outputs = model(**inputs)
            # Get the last hidden state
            last_hidden_states = outputs.last_hidden_state

        inference_time = (time.time() - start) * 1000

        result = {
            'model': 'bert',
            'inference_time_ms': inference_time,
            'input_text': text,
            'output_shape': list(last_hidden_states.shape),
            'embedding_dim': last_hidden_states.shape[-1]
        }

        print(json.dumps(result))

    except ImportError:
        print(json.dumps({
            'error': 'transformers not installed',
            'hint': 'pip install transformers'
        }))
        sys.exit(1)

def batch_inference(model_name='resnet50', batch_size=4):
    """Run batch inference to demonstrate throughput"""
    import torchvision.models as models

    model = models.resnet50(pretrained=True)
    model.eval()

    # Create batch of dummy inputs
    batch = torch.randn(batch_size, 3, 224, 224)

    # Warm up
    with torch.no_grad():
        _ = model(batch[:1])

    # Measure batch inference
    start = time.time()
    with torch.no_grad():
        outputs = model(batch)
    batch_time = (time.time() - start) * 1000

    # Measure individual inference
    start = time.time()
    for i in range(batch_size):
        with torch.no_grad():
            _ = model(batch[i:i+1])
    individual_time = (time.time() - start) * 1000

    result = {
        'model': model_name,
        'batch_size': batch_size,
        'batch_inference_ms': batch_time,
        'individual_inference_ms': individual_time,
        'speedup': individual_time / batch_time,
        'throughput_per_sec': (batch_size * 1000) / batch_time
    }

    print(json.dumps(result))

if __name__ == '__main__':
    import argparse

    parser = argparse.ArgumentParser(description='Run model inference')
    parser.add_argument('--model', default='resnet50',
                       choices=['resnet50', 'bert', 'batch'],
                       help='Model to use for inference')
    parser.add_argument('--text', default='Hello, world!',
                       help='Text for BERT inference')
    parser.add_argument('--batch-size', type=int, default=4,
                       help='Batch size for batch inference')

    args = parser.parse_args()

    if args.model == 'resnet50':
        run_resnet_inference()
    elif args.model == 'bert':
        run_bert_inference(args.text)
    elif args.model == 'batch':
        batch_inference(batch_size=args.batch_size)
    else:
        print(json.dumps({'error': f'Unknown model: {args.model}'}))
        sys.exit(1)