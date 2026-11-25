# OCR model assets

Bundled for offline inference with `rapidocr-core` 0.3.1 using the PP-OCRv6 Small model set and a detection + recognition pipeline (no direction classifier).

Source: RapidAI/RapidOCR model assets hosted on ModelScope, version `v3.9.0` for ONNX weights; dictionary from the project's `master` model asset path at integration time.

The OCR runtime does not download models at application startup. The three assets below are packaged with the desktop application and validated by `ModelCache` before the engine is initialized.

- `PP-OCRv6_det_small.onnx`
- `PP-OCRv6_rec_small.onnx`
- `ppocrv6_dict.txt`

The PP-OCRv6 model artifacts are Apache-2.0 according to RapidOCR’s model license statement. The Apache license copy and attribution notice are included alongside the assets. The dictionary is an upstream PaddleOCR resource and is attributed in `NOTICE.md`.

## SHA-256

- `PP-OCRv6_det_small.onnx`: `090f04abcd9d9a7498bc4ebf677e4cb9bdce1fe4197ddb7e529f1ef44e1ff94f`
- `PP-OCRv6_rec_small.onnx`: `6f327246b50388f3c176ae304bd95767ea6dc0c9ae92153ef8cbe210b3c14884`
- `ppocrv6_dict.txt`: `b5f2bfe2bdd9448429e3e82b51c789775d9b42f2403d082b00662eb77e401c5d`
