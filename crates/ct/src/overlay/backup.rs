use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;

const SLOTS: usize = D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE as usize;

pub struct Backup {
    rtvs: [Option<ID3D11RenderTargetView>; 8],
    dsv: Option<ID3D11DepthStencilView>,
    blend: Option<ID3D11BlendState>,
    blend_factor: [f32; 4],
    sample_mask: u32,
    depth: Option<ID3D11DepthStencilState>,
    stencil_ref: u32,
    rs: Option<ID3D11RasterizerState>,
    viewports: [D3D11_VIEWPORT; SLOTS],
    viewport_count: u32,
    scissors: [RECT; SLOTS],
    scissor_count: u32,
    layout: Option<ID3D11InputLayout>,
    topology: D3D_PRIMITIVE_TOPOLOGY,
    vb: Option<ID3D11Buffer>,
    vb_stride: u32,
    vb_offset: u32,
    ib: Option<ID3D11Buffer>,
    ib_format: DXGI_FORMAT,
    ib_offset: u32,
    vs: Option<ID3D11VertexShader>,
    vs_cb: [Option<ID3D11Buffer>; 1],
    ps: Option<ID3D11PixelShader>,
    ps_srv: [Option<ID3D11ShaderResourceView>; 1],
    ps_sampler: [Option<ID3D11SamplerState>; 1],
    gs: Option<ID3D11GeometryShader>,
    hs: Option<ID3D11HullShader>,
    ds: Option<ID3D11DomainShader>,
}

impl Backup {
    pub unsafe fn capture(c: &ID3D11DeviceContext) -> Self {
        let mut b = Self {
            rtvs: Default::default(),
            dsv: None,
            blend: None,
            blend_factor: [0.0; 4],
            sample_mask: 0,
            depth: None,
            stencil_ref: 0,
            rs: None,
            viewports: [D3D11_VIEWPORT::default(); SLOTS],
            viewport_count: SLOTS as u32,
            scissors: [RECT::default(); SLOTS],
            scissor_count: SLOTS as u32,
            layout: None,
            topology: D3D_PRIMITIVE_TOPOLOGY::default(),
            vb: None,
            vb_stride: 0,
            vb_offset: 0,
            ib: None,
            ib_format: DXGI_FORMAT::default(),
            ib_offset: 0,
            vs: None,
            vs_cb: Default::default(),
            ps: None,
            ps_srv: Default::default(),
            ps_sampler: Default::default(),
            gs: None,
            hs: None,
            ds: None,
        };
        unsafe {
            c.OMGetRenderTargets(Some(&mut b.rtvs), Some(&mut b.dsv));
            c.OMGetBlendState(Some(&mut b.blend), Some(&mut b.blend_factor), Some(&mut b.sample_mask));
            c.OMGetDepthStencilState(Some(&mut b.depth), Some(&mut b.stencil_ref));
            b.rs = c.RSGetState().ok();
            c.RSGetViewports(&mut b.viewport_count, Some(b.viewports.as_mut_ptr()));
            c.RSGetScissorRects(&mut b.scissor_count, Some(b.scissors.as_mut_ptr()));
            b.layout = c.IAGetInputLayout().ok();
            b.topology = c.IAGetPrimitiveTopology();
            c.IAGetVertexBuffers(0, 1, Some(&mut b.vb), Some(&mut b.vb_stride), Some(&mut b.vb_offset));
            c.IAGetIndexBuffer(Some(&mut b.ib), Some(&mut b.ib_format), Some(&mut b.ib_offset));
            c.VSGetShader(&mut b.vs, None, None);
            c.VSGetConstantBuffers(0, Some(&mut b.vs_cb));
            c.PSGetShader(&mut b.ps, None, None);
            c.PSGetShaderResources(0, Some(&mut b.ps_srv));
            c.PSGetSamplers(0, Some(&mut b.ps_sampler));
            c.GSGetShader(&mut b.gs, None, None);
            c.HSGetShader(&mut b.hs, None, None);
            c.DSGetShader(&mut b.ds, None, None);
            c.GSSetShader(None, None);
            c.HSSetShader(None, None);
            c.DSSetShader(None, None);
        }
        b
    }

    pub unsafe fn restore(self, c: &ID3D11DeviceContext) {
        unsafe {
            c.OMSetRenderTargets(Some(&self.rtvs), self.dsv.as_ref());
            c.OMSetBlendState(self.blend.as_ref(), Some(&self.blend_factor), self.sample_mask);
            c.OMSetDepthStencilState(self.depth.as_ref(), self.stencil_ref);
            c.RSSetState(self.rs.as_ref());
            c.RSSetViewports(Some(&self.viewports[..self.viewport_count as usize]));
            c.RSSetScissorRects(Some(&self.scissors[..self.scissor_count as usize]));
            c.IASetInputLayout(self.layout.as_ref());
            c.IASetPrimitiveTopology(self.topology);
            c.IASetVertexBuffers(0, 1, Some(&self.vb), Some(&self.vb_stride), Some(&self.vb_offset));
            c.IASetIndexBuffer(self.ib.as_ref(), self.ib_format, self.ib_offset);
            c.VSSetShader(self.vs.as_ref(), None);
            c.VSSetConstantBuffers(0, Some(&self.vs_cb));
            c.PSSetShader(self.ps.as_ref(), None);
            c.PSSetShaderResources(0, Some(&self.ps_srv));
            c.PSSetSamplers(0, Some(&self.ps_sampler));
            c.GSSetShader(self.gs.as_ref(), None);
            c.HSSetShader(self.hs.as_ref(), None);
            c.DSSetShader(self.ds.as_ref(), None);
        }
    }
}
