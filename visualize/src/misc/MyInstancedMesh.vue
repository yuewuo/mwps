<script lang="ts">
import { defineComponent, watchEffect } from 'vue'
import { InstancedMesh } from 'three'
import { Mesh, bindProp } from 'troisjs'

export default defineComponent({
    extends: Mesh,
    props: {
        count: { type: Number, required: true },
        maxcount: { type: Number },
        myData: { type: Object },
    },
    emits: ['reinstantiated'],
    data() {
        const current_count = this.maxcount == undefined ? this.count : this.maxcount
        return {
            current_count,
        }
    },
    methods: {
        initMesh() {
            this.originalInitMesh()

            // when `this.count` changes, update count (and instancedMesh if necessary)
            watchEffect(() => {
                if (this.count > this.current_count) {
                    if (this.count > 100) {
                        console.warn(`display (${this.count} objects more than ${this.current_count}, reconstructing...`)
                    }
                    // dispose current mesh (see troisjs/src/core/Object3D.ts), without disposing materials and geometries
                    if (!this.disableRemove) this.removeFromParent()
                    if (this.o3d) {
                        if (this.renderer) this.renderer.three.removeIntersectObject?.(this.o3d)
                    }
                    // create new mesh
                    this.current_count = this.count
                    this.originalInitMesh()
                    // emit the event of re-instantiating
                    this.$emit('reinstantiated', this.o3d)
                }
                // update the mesh with new count so that only the first `this.count` ones are visible
                ;(this.mesh as InstancedMesh).count = this.count
            })
        },
        originalInitMesh() {
            if (!this.renderer) return

            if (!this.geometry || !this.material) {
                console.error('Missing geometry and/or material')
                return false
            }

            this.mesh = new InstancedMesh(this.geometry, this.material, this.current_count)
            this.mesh.userData.component = this
            this.mesh.userData.myData = this.myData

            bindProp(this, 'castShadow', this.mesh)
            bindProp(this, 'receiveShadow', this.mesh)

            if (this.onPointerEnter || this.onPointerOver || this.onPointerMove || this.onPointerLeave || this.onPointerDown || this.onPointerUp || this.onClick) {
                this.renderer.three.addIntersectObject(this.mesh)
            }

            this.initObject3D(this.mesh)
        },
    },
    __hmrId: 'MyInstancedMesh',
})
</script>
