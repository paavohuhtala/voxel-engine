# RenderDoc format definitions

### PackedVoxelFace

```c
struct PackedVoxelFace {
	uint x : 4,
	uint y : 4,
	uint z : 4,
	uint face_id : 3,
	uint flip_diagonal : 1,
	uint width_m1 : 4,
	uint height_m1 : 4,
	uint ao_bl : 2,
	uint ao_br : 2,
	uint ao_tr : 2,
	uint ao_tl : 2,
	uint texture_index : 16,
	uint unused : 16
}
```
